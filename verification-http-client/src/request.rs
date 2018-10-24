// (c) Copyright 2018 Palantir Technologies Inc. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use bytes::BytesMut;
use errors::{Error, Result};
use futures::sync::oneshot;
use futures::{Async, Future, Poll};
use http_zipkin;
use hyper::body::{Chunk, Sender};
use hyper::header::{
    HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, HOST,
    PROXY_AUTHORIZATION, USER_AGENT,
};
use hyper::{self, HeaderMap, Method, StatusCode};
use std::collections::HashMap;
use std::io::{self, Write};
use std::result;
use std::thread;
use std::time::{Duration, SystemTime};
use typed_headers::{
    Authorization, ContentLength, ContentType, Credentials, HeaderMapExt, Host, RetryAfter, Token68,
};
use url::Url;
use zipkin::{Endpoint, Kind, TraceContext};

use async::custom_error::ConnectError;
use backoff::BackoffIterator;
use node_selector::Node;
use {Body, Client, ClientState, IntoBody, ProxyState, Response, RUNTIME};

lazy_static! {
    static ref DEFAULT_ACCEPT: HeaderValue =
        HeaderValue::from_static("*/*; q=0.5, application/cbor");
    static ref DEFAULT_ACCEPT_ENCODING: HeaderValue = HeaderValue::from_static("gzip, deflate");
}

pub struct RequestBuilder<'a> {
    pub(crate) client: &'a Client,
    pub(crate) method: Method,
    pub(crate) pattern: &'static str,
    pub(crate) params: HashMap<String, Vec<String>>,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Option<Result<Box<Body + 'a>>>,
    pub(crate) idempotent: bool,
}

impl<'a> RequestBuilder<'a> {
    pub(crate) fn new(
        client: &'a Client,
        pattern: &'static str,
        method: Method,
    ) -> RequestBuilder<'a> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, DEFAULT_ACCEPT.clone());
        headers.insert(ACCEPT_ENCODING, DEFAULT_ACCEPT_ENCODING.clone());
        headers.insert(USER_AGENT, client.user_agent.clone());

        RequestBuilder {
            client,
            pattern,
            idempotent: method.is_idempotent(),
            method,
            headers,
            body: None,
            params: HashMap::new(),
        }
    }

    /// Returns a mutable reference to the headers of this request.
    ///
    /// The following headers are set by default, but can be overridden:
    ///
    /// * `Accept-Encoding: gzip, deflate`
    /// * `Accept: */*; q=0.5, application/cbor`
    /// * `User-Agent: <provided at Client construction>`
    ///
    /// The following headers are fully controlled by Chatter, which will overwrite any existing value.
    ///
    /// * `Connection`
    /// * `Content-Length`
    /// * `Content-Type`
    /// * `Host`
    /// * `Proxy-Authorization`
    /// * `X-B3-Flags`
    /// * `X-B3-ParentSpanId`
    /// * `X-B3-Sampled`
    /// * `X-B3-SpanId`
    /// * `X-B3-TraceId`
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    /// Sets the `Authorization` request header to a bearer token.
    ///
    /// This is a simple convenience wrapper.
    pub fn bearer_token(&mut self, token: &str) -> &mut RequestBuilder<'a> {
        let token = Token68::new(token).expect("invalid bearer token");
        let credentials = Credentials::bearer(token);
        let value = Authorization(credentials);
        self.headers.typed_insert(&value);
        self
    }

    /// Adds a parameter.
    ///
    /// Parameters which match names in the path pattern will be treated as
    /// path parameters, and other parameters will be treated as query
    /// parameters. Only one instance of path parameters may be provided, but
    /// multiple instances of query parameters may be provided.
    pub fn param<T>(&mut self, name: &str, value: T) -> &mut RequestBuilder<'a>
    where
        T: ToString,
    {
        self.params
            .entry(name.to_string())
            .or_insert_with(Vec::new)
            .push(value.to_string());
        self
    }

    /// Sets the idempotency of the request.
    ///
    /// Idempotent requests can be retried if an IO error is encountered.
    ///
    /// This is by default derived from the HTTP method. `GET`, `HEAD`,
    /// `OPTIONS`, `TRACE`, `PUT`, and `DELETE` are defined as idempotent.
    pub fn idempotent(&mut self, idempotent: bool) -> &mut RequestBuilder<'a> {
        self.idempotent = idempotent;
        self
    }

    /// Sets the request body.
    pub fn body<T>(&mut self, body: T) -> &mut RequestBuilder<'a>
    where
        T: IntoBody,
        T::Target: 'a,
    {
        self.body = Some(body.into_body().map(|b| Box::new(b) as Box<Body>));
        self
    }

    /// Makes the request.
    ///
    /// Non-successful (2xx) status codes are treated as errors.
    ///
    /// # Panics
    ///
    /// Panics if any path parameters are missing.
    pub fn send(&mut self) -> Result<Response> {
        let state = self.client.get_refresh();

        let mut nodes = match state.nodes.iter() {
            Some(nodes) => nodes,
            None => {
                return Err(Error::internal_safe("service has no configured URLs")
                    .with_safe_param("service", &self.client.service))
            }
        };

        let mut body = match self.body.take() {
            Some(Ok(body)) => Some(body),
            Some(Err(e)) => return Err(e),
            None => None,
        };

        let mut node = nodes.get();
        let mut backoffs = BackoffIterator::new(state.max_num_retries, state.backoff_slot_size);

        loop {
            let (backoff, reset_body, change_node) =
                match self.send_once(node, &state, body.as_mut()) {
                    Ok(response) => {
                        nodes.set_default();
                        return Ok(response);
                    }
                    Err(SendError::Throttle { backoff }) => {
                        let internal_backoff = backoffs.next().ok_or_else(|| {
                            Error::internal_safe("exceeded max retry limit after 429")
                        })?;

                        let backoff = backoff.unwrap_or(internal_backoff);

                        (backoff, true, false)
                    }
                    Err(SendError::Unavailable) => {
                        let backoff = backoffs.next().ok_or_else(|| {
                            Error::internal_safe("exceeded max retry limit after 503")
                        })?;

                        (backoff, true, true)
                    }
                    Err(SendError::Other { error }) => return Err(error),
                    Err(SendError::Io { error, reset_body }) => {
                        let backoff = match backoffs.next() {
                            Some(backoff) => backoff,
                            None => return Err(error),
                        };

                        info!(
                            "attempting to retry after IO error. url: {}, error: {}",
                            node.url, error
                        );

                        (backoff, reset_body, true)
                    }
                };

            if change_node {
                node = nodes.next();
            }

            if !self.idempotent {
                return Err(Error::internal_safe(
                    "unable to retry non-idempotent request",
                ));
            }

            if reset_body {
                if let Some(ref mut body) = body {
                    if !body.reset() {
                        return Err(Error::internal_safe(
                            "unable to reset body when retrying request",
                        ));
                    }
                }
            }

            info!("retrying call after backoff {}ms", micros(backoff));
            thread::sleep(backoff);
        }
    }

    fn send_once(
        &mut self,
        node: &Node,
        state: &ClientState,
        body: Option<&mut Box<Body + 'a>>,
    ) -> result::Result<Response, SendError> {
        match self.send_traced(node, state, body) {
            Ok(response) => {
                let status = response.status();

                if status.is_success() {
                    Ok(response)
                } else if status == StatusCode::TOO_MANY_REQUESTS {
                    let backoff = match response.headers().typed_get() {
                        Ok(Some(RetryAfter::DelaySeconds(s))) => Some(Duration::from_secs(s)),
                        Ok(Some(RetryAfter::HttpDate(date))) => SystemTime::from(date)
                            .duration_since(SystemTime::now())
                            .ok(),
                        _ => None,
                    };
                    Err(SendError::Throttle { backoff })
                } else if status == StatusCode::SERVICE_UNAVAILABLE {
                    Err(SendError::Unavailable)
                } else {
                    Err(SendError::Other {
                        error: response.into_error(),
                    })
                }
            }
            Err(RawError::Connect(error)) => Err(SendError::Io {
                error,
                reset_body: false,
            }),
            Err(RawError::Other(error)) => Err(SendError::Io {
                error,
                reset_body: true,
            }),
        }
    }

    fn send_traced(
        &mut self,
        node: &Node,
        state: &ClientState,
        body: Option<&mut Box<Body + 'a>>,
    ) -> result::Result<Response, RawError> {
        let mut span = self.client.tracer.next_span();
        span.name(&format!("{} {}", self.method, self.pattern));
        span.tag("http.method", &self.method.to_string());
        span.tag("http.path", self.pattern);
        span.kind(Kind::Client);
        // FIXME once we have more control over hyper we should attach the IP/port
        span.remote_endpoint(
            Endpoint::builder()
                .service_name(&self.client.service)
                .build(),
        );

        let r = self.send_raw(&node.url, &state, span.context(), body);

        let status = match r {
            Ok(ref response) => Some(response.status()),
            Err(_) => None,
        };

        if let Some(status) = status {
            if !status.is_success() {
                span.tag("http.status_code", &status.to_string());
            }
        }

        r
    }

    fn send_raw(
        &mut self,
        url: &Url,
        state: &ClientState,
        context: TraceContext,
        body: Option<&mut Box<Body + 'a>>,
    ) -> result::Result<Response, RawError> {
        let mut url = self.build_url(url);

        let mut headers = self.headers.clone();
        headers.remove(&CONNECTION);
        headers.remove(&HOST);
        headers.remove(&PROXY_AUTHORIZATION);
        headers.remove(&CONTENT_LENGTH);
        headers.remove(&CONTENT_TYPE);
        http_zipkin::set_trace_context(context, &mut headers);

        match state.proxy {
            Some(ProxyState::Http { ref credentials }) => {
                if url.scheme() == "http" {
                    if let Some(ref credentials) = *credentials {
                        headers.typed_insert(credentials);
                    }
                }
            }
            Some(ProxyState::Mesh { ref host }) => {
                let header = Host::new(url.host_str().unwrap(), url.port())
                    .expect("url host should be valid");
                headers.typed_insert(&header);
                url.set_host(Some(host.host())).unwrap();
                url.set_port(Some(host.port())).unwrap();
            }
            None => {}
        }

        let (body, hyper_body) = match body {
            Some(body) => {
                if let Some(length) = body.content_length() {
                    headers.typed_insert(&ContentLength(length));
                }
                headers.typed_insert(&ContentType(body.content_type()));

                match body.full_body() {
                    Some(body) => (None, hyper::Body::from(body)),
                    None => {
                        let (sender, hyper_body) = hyper::Body::channel();
                        (Some((body, sender)), hyper_body)
                    }
                }
            }
            None => (None, hyper::Body::empty()),
        };

        let mut request = hyper::Request::new(hyper_body);
        *request.method_mut() = self.method.clone();
        *request.uri_mut() = url.as_str().parse().unwrap();
        *request.headers_mut() = headers;

        let response = oneshot::spawn(state.client.request(request), &RUNTIME.executor());

        if let Some((body, sender)) = body {
            let mut writer = BodyWriter {
                sender: Some(sender),
                buf: BytesMut::new(),
            };
            body.write(&mut writer).map_err(RawError::Other)?;
            writer.finish();
        }

        match response.wait() {
            Ok(response) => Ok(Response::new(response)),
            Err(e) => {
                if e.cause2().map_or(false, |e| e.is::<ConnectError>()) {
                    Err(RawError::Connect(Error::internal_safe(e)))
                } else {
                    Err(RawError::Other(Error::internal_safe(e)))
                }
            }
        }
    }

    fn build_url(&self, url: &Url) -> Url {
        let mut url = url.clone();
        let mut params = self.params.clone();

        assert!(self.pattern.starts_with("/"), "pattern must start with `/`");
        // make sure to skip the leading `/` to avoid an empty path segment
        for segment in self.pattern[1..].split("/") {
            if segment.starts_with(":") {
                let name = &segment[1..];
                match params.remove(name) {
                    Some(ref values) if values.len() != 1 => {
                        panic!("path segment parameter {} had multiple values", name);
                    }
                    Some(value) => {
                        url.path_segments_mut().unwrap().push(&value[0]);
                    }
                    None => panic!("path segment parameter {} had no values", name),
                }
            } else if segment.starts_with("*") {
                let name = &segment[1..];
                match params.remove(name) {
                    Some(ref values) if values.len() != 1 => {
                        panic!("path segment parameter {} had multiple values", name);
                    }
                    Some(value) => {
                        // * patterns can match multiple segments so split the value to avoid
                        // encoding the path separators
                        for value in value[0].split("/") {
                            url.path_segments_mut().unwrap().push(value);
                        }
                    }
                    None => panic!("path segment parameter {} had no values", name),
                }
            } else {
                url.path_segments_mut().unwrap().push(segment);
            }
        }

        for (k, vs) in &params {
            for v in vs {
                url.query_pairs_mut().append_pair(k, v);
            }
        }

        url
    }
}

struct BodyWriter {
    sender: Option<Sender>,
    buf: BytesMut,
}

impl Drop for BodyWriter {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            sender.abort();
        }
    }
}

impl BodyWriter {
    fn finish(&mut self) {
        self.flush_inner();
        self.sender = None;
    }

    fn flush_inner(&mut self) {
        if self.buf.len() == 0 {
            return;
        }

        let hup = match self.sender {
            Some(ref mut sender) => {
                let mut future = SendFuture {
                    sender,
                    data: Some(Chunk::from(self.buf.take().freeze())),
                };
                match future.wait() {
                    Ok(()) => false,
                    Err(_) => {
                        // we'll get an error/whatever when reading the response, so silence this error
                        info!("server hung up while streaming body");
                        true
                    }
                }
            }
            None => false,
        };

        if hup {
            self.sender = None;
        }
    }
}

impl Write for BodyWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.sender.is_none() {
            return Ok(buf.len());
        }

        self.buf.extend_from_slice(buf);
        if self.buf.len() > 4096 {
            self.flush_inner();
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_inner();

        Ok(())
    }
}

struct SendFuture<'a> {
    sender: &'a mut Sender,
    data: Option<Chunk>,
}

impl<'a> Future for SendFuture<'a> {
    type Item = ();
    type Error = hyper::Error;

    fn poll(&mut self) -> Poll<(), hyper::Error> {
        loop {
            try_ready!(self.sender.poll_ready());

            let data = self.data.take().expect("future polled after completion");
            match self.sender.send_data(data) {
                Ok(()) => return Ok(Async::Ready(())),
                Err(data) => self.data = Some(data),
            }
        }
    }
}

enum RawError {
    Connect(Error),
    Other(Error),
}

enum SendError {
    Throttle { backoff: Option<Duration> },
    Unavailable,
    Other { error: Error },
    Io { error: Error, reset_body: bool },
}

fn micros(d: Duration) -> u64 {
    d.as_secs() * 1_000_000 + d.subsec_nanos() as u64 / 1_000
}
