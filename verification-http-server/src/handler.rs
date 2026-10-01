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

use crate::error_handling;
use crate::router::Endpoint;
use crate::router::RouteResult;
use crate::router::Router;
use bytes::{Bytes, BytesMut};
use conjure_verification_error::Code;
use conjure_verification_error::Error;
use conjure_verification_error::Result;
use conjure_verification_http::error::ConjureVerificationError;
use conjure_verification_http::headers::{self, Encoding};
use conjure_verification_http::request::Request;
use conjure_verification_http::response::*;
use flate2::bufread::{GzDecoder, ZlibDecoder};
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::Frame;
use hyper::{HeaderMap, StatusCode, Uri};
use itertools::Itertools;
use log::Level;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use url::form_urlencoded;

type BoxError = Box<dyn StdError + Sync + Send>;

type IncomingBody = hyper::body::Incoming;

/// Error type returned by the HTTP service.
#[derive(Debug)]
pub struct HttpServiceError(BoxError);

impl std::fmt::Display for HttpServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl StdError for HttpServiceError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&*self.0)
    }
}

#[derive(Clone)]
pub struct HttpService {
    router: Arc<Router>,
}

impl HttpService {
    pub fn new(router: Arc<Router>) -> HttpService {
        HttpService { router }
    }

    fn route(&self, request: &hyper::Request<IncomingBody>) -> RouteResult {
        let path = &request.uri().path();
        self.router.route(request.method(), path)
    }

    fn query_params(&self, uri: &Uri) -> HashMap<String, Vec<String>> {
        let mut params = HashMap::new();
        if let Some(query) = uri.query() {
            for (k, v) in form_urlencoded::parse(query.as_bytes()) {
                params
                    .entry(k.to_string())
                    .or_insert_with(Vec::new)
                    .push(v.to_string());
            }
        }
        params
    }

    fn path_params(&self, route: &RouteResult) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        if let RouteResult::Matched { ref params, .. } = *route {
            for (k, v) in params {
                let value = percent_encoding::percent_decode_str(v)
                    .decode_utf8()
                    .map_err(|e| Error::new_safe(e, ConjureVerificationError::InvalidUrl))?;

                map.insert(k.to_string(), value.to_string());
            }
        }
        Ok(map)
    }

    pub fn call<'a>(
        &'a self,
        request: hyper::Request<IncomingBody>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = std::result::Result<hyper::Response<ResponseBody>, HttpServiceError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let route = self.route(&request);
            let query_params = self.query_params(request.uri());
            let maybe_path_params = self.path_params(&route);

            let response = self
                .response(request, route, maybe_path_params, query_params)
                .await
                .map_err(HttpServiceError)?;
            Ok(response)
        })
    }

    async fn response(
        &self,
        request: hyper::Request<IncomingBody>,
        route: RouteResult,
        path_params: Result<HashMap<String, String>>,
        query_params: HashMap<String, Vec<String>>,
    ) -> std::result::Result<hyper::Response<ResponseBody>, BoxError> {
        match (route, path_params) {
            (RouteResult::NotFound, _) => {
                info!("unrouted request: {}", request.uri());
                let mut response = hyper::Response::new(ResponseBody::empty());
                *response.status_mut() = StatusCode::NOT_FOUND;
                Ok(response)
            }
            (RouteResult::MethodNotAllowed(methods), _) => {
                let display_methods = methods.iter().join(", ");
                info!(
                    "method not allowed. method: {}, allowed_methods: {}.",
                    request.method(),
                    display_methods
                );
                let mut response = hyper::Response::new(ResponseBody::empty());
                *response.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
                headers::set_allow(response.headers_mut(), &methods);
                Ok(response)
            }
            (_, Err(e)) => {
                info!("Improperly formatted URL. Error: {}", e);
                let mut response = hyper::Response::new(ResponseBody::empty());
                *response.status_mut() = StatusCode::NOT_FOUND;
                Ok(response)
            }
            (RouteResult::Matched { endpoint, .. }, Ok(path_params)) => {
                // Buffer the incoming request body so the sync handler can
                // read it via the blocking thread pool.
                let (parts, body) = request.into_parts();
                let body_bytes = body.collect().await?.to_bytes();

                let response_size = Arc::new(AtomicUsize::new(0));
                let (sender, receiver) = oneshot::channel();

                // The handler sends the response head through `sender`, then keeps running to stream the body.
                let sync = SyncHandler;
                tokio::task::spawn_blocking(move || {
                    sync.response(
                        parts.headers,
                        body_bytes,
                        endpoint,
                        path_params,
                        query_params,
                        sender,
                        &response_size,
                    )
                });

                match receiver.await {
                    Ok((response, _request_size)) => Ok(response),
                    Err(_) => {
                        error!("handler thread hung up");
                        let mut response = hyper::Response::new(ResponseBody::empty());
                        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                        Ok(response)
                    }
                }
            }
        }
    }
}

/// The response body type used by the HTTP service. Streaming bodies are
/// supported through a channel.
pub enum ResponseBody {
    Full(Full<Bytes>),
    Stream(StreamBody<ReceiverStream<io::Result<Frame<Bytes>>>>),
}

impl ResponseBody {
    fn empty() -> ResponseBody {
        ResponseBody::Full(Full::new(Bytes::new()))
    }
}

impl hyper::body::Body for ResponseBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<std::result::Result<hyper::body::Frame<Self::Data>, Self::Error>>>
    {
        match self.get_mut() {
            ResponseBody::Full(b) => {
                let p = std::pin::Pin::new(b);
                match p.poll_frame(cx) {
                    std::task::Poll::Ready(Some(Ok(frame))) => {
                        std::task::Poll::Ready(Some(Ok(frame)))
                    }
                    std::task::Poll::Ready(Some(Err(e))) => {
                        std::task::Poll::Ready(Some(Err(Box::new(e))))
                    }
                    std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
                    std::task::Poll::Pending => std::task::Poll::Pending,
                }
            }
            ResponseBody::Stream(b) => {
                let p = std::pin::Pin::new(b);
                match p.poll_frame(cx) {
                    std::task::Poll::Ready(Some(Ok(frame))) => {
                        std::task::Poll::Ready(Some(Ok(frame)))
                    }
                    std::task::Poll::Ready(Some(Err(e))) => std::task::Poll::Ready(Some(Err(
                        Box::new(io::Error::new(io::ErrorKind::Other, e)) as BoxError,
                    ))),
                    std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
                    std::task::Poll::Pending => std::task::Poll::Pending,
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ResponseBody::Full(b) => b.is_end_stream(),
            ResponseBody::Stream(b) => b.is_end_stream(),
        }
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        match self {
            ResponseBody::Full(b) => b.size_hint(),
            ResponseBody::Stream(b) => b.size_hint(),
        }
    }
}

struct SyncHandler;

impl SyncHandler {
    fn response(
        &self,
        headers: HeaderMap,
        body_bytes: Bytes,
        endpoint: Arc<Endpoint>,
        path_params: HashMap<String, String>,
        query_params: HashMap<String, Vec<String>>,
        sender: ResponseSender,
        response_size: &Arc<AtomicUsize>,
    ) {
        let body = Cursor::new(body_bytes);
        let mut body = SizeTrackingReader {
            reader: body,
            size: 0,
        };

        let response = self
            .response_inner(&headers, &mut body, &endpoint, &path_params, &query_params)
            .unwrap_or_else(|e| self.handler_error(&e));

        self.write_response(&headers, response, body.size, sender, response_size);
    }

    fn handler_error(&self, e: &Error) -> Response {
        let r = error_handling::response(&e);
        let level = match r.status {
            StatusCode::INTERNAL_SERVER_ERROR => Level::Error,
            _ => Level::Info,
        };
        log!(level, "handler returned non-success. Error: {}", e);
        r
    }

    fn response_inner(
        &self,
        headers: &HeaderMap,
        body: &mut SizeTrackingReader<Cursor<Bytes>>,
        endpoint: &Arc<Endpoint>,
        path_params: &HashMap<String, String>,
        query_params: &HashMap<String, Vec<String>>,
    ) -> Result<Response> {
        let mut body = self.decode_body(&headers, body)?;
        let mut request = Request::new(&path_params, &query_params, &headers, &mut *body);

        endpoint.handler.handle(&mut request)
    }

    fn decode_body<'a>(
        &self,
        headers: &HeaderMap,
        body: &'a mut SizeTrackingReader<Cursor<Bytes>>,
    ) -> Result<Box<dyn Read + 'a>> {
        match headers::get_content_encoding(headers) {
            Ok(encoding) => match encoding.as_slice() {
                [] | [Encoding::Identity] => Ok(Box::new(body)),
                [Encoding::Gzip] => Ok(Box::new(BufReader::new(GzDecoder::new(body)))),
                [Encoding::Deflate] => Ok(Box::new(BufReader::new(ZlibDecoder::new(body)))),
                // this forbids encodings we "could" support like `gzip, deflate, identity, gzip`, but that's a
                // dumb thing to try to use
                _ => Err(Error::new_safe(
                    "unsupported Content-Encoding",
                    Code::CustomClient,
                )),
            },
            Err(e) => Err(Error::new_safe(e, Code::CustomClient)),
        }
    }

    fn write_response(
        &self,
        headers: &HeaderMap,
        raw_response: Response,
        request_size: u64,
        sender: ResponseSender,
        response_size: &Arc<AtomicUsize>,
    ) {
        let raw_response = self.handle_response_size(response_size, raw_response);

        let mut body = match raw_response.body {
            Body::Empty => {
                let mut response = hyper::Response::new(ResponseBody::empty());
                *response.status_mut() = raw_response.status;
                *response.headers_mut() = raw_response.headers;
                let _ = sender.send((response, request_size));
                return;
            }
            Body::Fixed(bytes) => {
                let mut response =
                    hyper::Response::new(ResponseBody::Full(Full::new(bytes.into())));
                *response.status_mut() = raw_response.status;
                *response.headers_mut() = raw_response.headers;
                let _ = sender.send((response, request_size));
                return;
            }
            Body::Streaming(body) => body,
        };

        let mut body_writer = BodyWriter {
            state: BodyWriterState::Buffering {
                request_size,
                status: raw_response.status,
                headers: raw_response.headers,
                sender,
            },
            buf: BytesMut::new(),
        };

        match body.write_body(&mut body_writer) {
            Ok(()) => body_writer.finish(),
            Err(e) => match mem::replace(&mut body_writer.state, BodyWriterState::Done) {
                // Nothing has been sent yet, so we can still respond with the error.
                BodyWriterState::Buffering {
                    request_size,
                    sender,
                    ..
                } => {
                    let raw_response = self.handler_error(&e);
                    self.write_response(headers, raw_response, request_size, sender, response_size);
                }
                state => {
                    // Dropping the writer aborts the partially sent response.
                    body_writer.state = state;
                    info!("error sending response. Error {}", e);
                }
            },
        }
    }

    fn handle_response_size(
        &self,
        response_size: &Arc<AtomicUsize>,
        mut response: Response,
    ) -> Response {
        response.body = match response.body {
            Body::Empty => {
                response_size.store(0, Ordering::SeqCst);
                Body::Empty
            }
            Body::Fixed(bytes) => {
                response_size.store(bytes.len(), Ordering::SeqCst);
                Body::Fixed(bytes)
            }
            Body::Streaming(write_body) => Body::Streaming(write_body),
        };

        response
    }
}

struct SizeTrackingReader<R> {
    reader: R,
    size: u64,
}

impl<R> Read for SizeTrackingReader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf).map(|n| {
            self.size += n as u64;
            n
        })
    }
}

impl<R> BufRead for SizeTrackingReader<R>
where
    R: BufRead,
{
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.reader.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.size += amt as u64;
        self.reader.consume(amt)
    }
}

type ResponseSender = oneshot::Sender<(hyper::Response<ResponseBody>, u64)>;
type BodySender = mpsc::Sender<io::Result<Frame<Bytes>>>;

enum BodyWriterState {
    Buffering {
        request_size: u64,
        status: StatusCode,
        headers: HeaderMap,
        sender: ResponseSender,
    },
    Writing {
        sender: BodySender,
    },
    Done,
}

/// Buffers the start of a streaming body, so that short bodies are sent with a `Content-Length` and errors raised
/// before anything has been sent can still produce an error response. Once more than 4KB has been written or the
/// body is flushed, the response head is sent and the rest of the body is streamed.
struct BodyWriter {
    state: BodyWriterState,
    buf: BytesMut,
}

impl Drop for BodyWriter {
    fn drop(&mut self) {
        // Abort a partially sent body rather than letting it end as if it were complete.
        if let BodyWriterState::Writing { sender } =
            mem::replace(&mut self.state, BodyWriterState::Done)
        {
            let _ = sender.blocking_send(Err(io::Error::new(
                io::ErrorKind::Other,
                "response body aborted",
            )));
        }
    }
}

impl BodyWriter {
    fn finish(&mut self) {
        match mem::replace(&mut self.state, BodyWriterState::Done) {
            BodyWriterState::Buffering {
                request_size,
                status,
                headers,
                sender,
            } => {
                let buf = self.buf.split().freeze();

                let body = if buf.is_empty() {
                    ResponseBody::empty()
                } else {
                    ResponseBody::Full(Full::new(buf))
                };

                let mut response = hyper::Response::new(body);
                *response.status_mut() = status;
                *response.headers_mut() = headers;
                let _ = sender.send((response, request_size));
            }
            BodyWriterState::Writing { sender } => {
                let _ = self.send_chunk(&sender);
            }
            BodyWriterState::Done => {}
        }
    }

    fn send_chunk(&mut self, sender: &BodySender) -> io::Result<()> {
        let buf = self.buf.split().freeze();
        if buf.is_empty() {
            return Ok(());
        }

        // blocking_send since we're on a blocking thread, not a tokio worker
        sender
            .blocking_send(Ok(Frame::data(buf)))
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }
}

impl Write for BodyWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(buf);
        if self.buf.len() > 4096 {
            self.flush()?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let sender = match mem::replace(&mut self.state, BodyWriterState::Done) {
            BodyWriterState::Buffering {
                request_size,
                status,
                headers,
                sender,
            } => {
                let (body_sender, receiver) = mpsc::channel(1);
                let body = ResponseBody::Stream(StreamBody::new(ReceiverStream::new(receiver)));

                let mut response = hyper::Response::new(body);
                *response.status_mut() = status;
                *response.headers_mut() = headers;

                match sender.send((response, request_size)) {
                    Ok(()) => body_sender,
                    Err(_) => return Ok(()),
                }
            }
            BodyWriterState::Writing { sender } => sender,
            BodyWriterState::Done => return Ok(()),
        };

        let r = self.send_chunk(&sender);
        self.state = BodyWriterState::Writing { sender };
        r
    }
}
