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
use conjure_verification_error::Code;
use conjure_verification_error::Error;
use conjure_verification_error::Result;
use conjure_verification_http::error::ConjureVerificationError;
use conjure_verification_http::request::Request;
use conjure_verification_http::response::*;
use core::mem;
use error_handling;
use flate2::bufread::{GzDecoder, ZlibDecoder};
use futures::future;
use futures::stream;
use futures::sync::oneshot;
use futures::Async;
use futures::Future;
use futures::Poll;
use futures::Stream;
use hyper::body;
use hyper::service::Service;
use hyper::{self, Chunk, HeaderMap, StatusCode, Uri};
use itertools::Itertools;
use log::Level;
use router::Endpoint;
use router::RouteResult;
use router::Router;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::executor::thread_pool::ThreadPool;
use typed_headers::{Allow, ContentCoding, ContentEncoding, HeaderMapExt};
use url::{form_urlencoded, percent_encoding};

pub struct HttpService {
    router: Arc<Router>,
    sync: Arc<SyncHandler>,
    pool: Arc<ThreadPool>,
}

impl HttpService {
    pub fn new(router: Arc<Router>) -> HttpService {
        HttpService {
            router,
            sync: Arc::new(SyncHandler),
            pool: Arc::new(ThreadPool::new()),
        }
    }

    fn route(&self, request: &hyper::Request<hyper::Body>) -> RouteResult {
        let path = &request.uri().path();
        self.router.route(request.method().clone(), path)
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
                let value = percent_encoding::percent_decode(v.as_bytes())
                    .decode_utf8()
                    .map_err(|e| Error::new_safe(e, ConjureVerificationError::InvalidUrl))?;

                map.insert(k.to_string(), value.to_string());
            }
        }
        Ok(map)
    }

    fn response(
        &self,
        request: hyper::Request<hyper::Body>,
        route: RouteResult,
        path_params: Result<HashMap<String, String>>,
        query_params: HashMap<String, Vec<String>>,
        response_size: Arc<AtomicUsize>,
    ) -> Box<
        Future<Item = (hyper::Response<hyper::Body>, u64), Error = Box<StdError + Sync + Send>>
            + Send,
    > {
        match (route, path_params) {
            (RouteResult::NotFound, _) => {
                info!("unrouted request: {}", request.uri());
                let mut response = hyper::Response::new(hyper::Body::empty());
                *response.status_mut() = StatusCode::NOT_FOUND;
                Box::new(future::ok((response, 0)))
            }
            (RouteResult::MethodNotAllowed(methods), _) => {
                let display_methods = methods.iter().join(", ");
                info!(
                    "method not allowed. method: {}, allowed_methods: {}.",
                    request.method(),
                    display_methods
                );
                let mut response = hyper::Response::new(hyper::Body::empty());
                *response.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
                response.headers_mut().typed_insert(&Allow(methods));
                Box::new(future::ok((response, 0)))
            }
            (_, Err(e)) => {
                info!("Improperly formatted URL. Error: {}", e);
                let mut response = hyper::Response::new(hyper::Body::empty());
                *response.status_mut() = StatusCode::NOT_FOUND;
                Box::new(future::ok((response, 0)))
            }
            (RouteResult::Matched { endpoint, .. }, Ok(path_params)) => {
                let (sender, receiver) = oneshot::channel();

                let sync = self.sync.clone();
                let r = self.pool.sender().spawn(future::lazy(move || {
                    sync.response(
                        request,
                        endpoint,
                        path_params,
                        query_params,
                        sender,
                        response_size,
                    );
                    Ok(())
                }));

                match r {
                    Ok(()) => {
                        let f = receiver.or_else(|_| {
                            error!("handler thread hung up");
                            let mut response = hyper::Response::new(hyper::Body::empty());
                            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                            Box::new(future::ok((response, 0)))
                        });
                        Box::new(f)
                    }
                    Err(_) => {
                        let mut response = hyper::Response::new(hyper::Body::empty());
                        *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
                        Box::new(future::ok((response, 0)))
                    }
                }
            }
        }
    }
}

struct SyncHandler;

impl SyncHandler {
    fn response(
        &self,
        request: hyper::Request<hyper::Body>,
        endpoint: Arc<Endpoint>,
        path_params: HashMap<String, String>,
        query_params: HashMap<String, Vec<String>>,
        sender: oneshot::Sender<(hyper::Response<hyper::Body>, u64)>,
        response_size: Arc<AtomicUsize>,
    ) {
        let (parts, body) = request.into_parts();

        let body = BodyReader {
            it: body.wait(),
            cur: Cursor::new(Chunk::from("")),
        };
        let mut body = SizeTrackingReader {
            reader: body,
            size: 0,
        };

        let response = self
            .response_inner(
                &parts.headers,
                &mut body,
                endpoint,
                path_params,
                query_params,
            ).unwrap_or_else(|e| self.handler_error(e));

        self.write_response(&parts.headers, response, body.size, sender, &response_size);
    }

    fn handler_error(&self, e: Error) -> Response {
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
        body: &mut SizeTrackingReader<BodyReader>,
        endpoint: Arc<Endpoint>,
        path_params: HashMap<String, String>,
        query_params: HashMap<String, Vec<String>>,
    ) -> Result<Response> {
        let mut body = self.decode_body(&headers, body)?;
        let mut request = Request::new(&path_params, &query_params, &headers, &mut *body);

        endpoint.handler.handle(&mut request)
    }

    fn decode_body<'a>(
        &self,
        headers: &HeaderMap,
        body: &'a mut SizeTrackingReader<BodyReader>,
    ) -> Result<Box<Read + 'a>> {
        match headers.typed_get::<ContentEncoding>() {
            Ok(Some(encoding)) => {
                match &**encoding {
                    [] | [ContentCoding::IDENTITY] => Ok(Box::new(body)),
                    [ContentCoding::GZIP] => Ok(Box::new(BufReader::new(GzDecoder::new(body)))),
                    [ContentCoding::DEFLATE] => {
                        Ok(Box::new(BufReader::new(ZlibDecoder::new(body))))
                    }
                    // this forbids encodings we "could" support like `gzip, deflate, identity, gzip`, but that's a
                    // dumb thing to try to use
                    _ => Err(Error::new_safe(
                        "unsupported Content-Encoding",
                        Code::CustomClient,
                    )),
                }
            }
            Ok(None) => Ok(Box::new(body)),
            Err(e) => Err(Error::new_safe(e, Code::CustomClient)),
        }
    }

    fn write_response(
        &self,
        headers: &HeaderMap,
        raw_response: Response,
        request_size: u64,
        sender: oneshot::Sender<(hyper::Response<hyper::Body>, u64)>,
        response_size: &Arc<AtomicUsize>,
    ) {
        let raw_response = self.handle_response_size(response_size, raw_response);
        // TODO(dsanduleac): don't wanna encode
        //        let raw_response = encode::encode(headers, raw_response);

        let mut body = match raw_response.body {
            Body::Empty => {
                let mut response = hyper::Response::new(hyper::Body::empty());
                *response.status_mut() = raw_response.status;
                *response.headers_mut() = raw_response.headers;
                let _ = sender.send((response, request_size));
                return;
            }
            Body::Fixed(bytes) => {
                response_size.store(bytes.len(), Ordering::SeqCst);
                let mut response = hyper::Response::new(hyper::Body::from(bytes));
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

        let r = match body.write_body(&mut body_writer) {
            Ok(()) => Ok(()),
            Err(e) => {
                if let BodyWriterState::Buffering {
                    request_size,
                    sender,
                    ..
                } = mem::replace(&mut body_writer.state, BodyWriterState::Done)
                {
                    let raw_response = self.handler_error(e);
                    return self.write_response(
                        headers,
                        raw_response,
                        request_size,
                        sender,
                        response_size,
                    );
                }

                Err(e)
            }
        };

        match r {
            Ok(()) => {
                body_writer.finish();
            }
            Err(e) => {
                info!("error sending response. Error {}", e);
            }
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
            Body::Streaming(_write_body) =>
                unimplemented!()
//                Body::Streaming(Box::new(SizeTrackingWriteBody {
//                write_body: write_body,
//                response_size: response_size.clone(),
//            })),
        };

        response
    }
}

impl Service for HttpService {
    type ReqBody = hyper::Body;
    type ResBody = hyper::Body;
    type Error = Box<StdError + Sync + Send>;
    type Future = Box<
        Future<Item = hyper::Response<hyper::Body>, Error = Box<StdError + Sync + Send>> + Send,
    >;

    fn call(
        &mut self,
        request: hyper::Request<<Self as Service>::ReqBody>,
    ) -> Box<Future<Item = hyper::Response<hyper::Body>, Error = Box<StdError + Sync + Send>> + Send>
    {
        let route = self.route(&request);
        let query_params = self.query_params(request.uri());
        let maybe_path_params = self.path_params(&route);
        let response_size = Arc::new(AtomicUsize::new(0));

        let f = self
            .response(
                request,
                route,
                maybe_path_params,
                query_params,
                response_size,
            ).map({ move |(response, _request_size)| response });

        Box::new(f)
    }
}

struct BodyReader {
    it: stream::Wait<hyper::Body>,
    cur: Cursor<hyper::Chunk>,
}

impl Read for BodyReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let nread = {
            let read_buf = self.fill_buf()?;
            let nread = usize::min(buf.len(), read_buf.len());
            buf[..nread].copy_from_slice(&read_buf[..nread]);
            nread
        };
        self.consume(nread);
        Ok(nread)
    }
}

impl BufRead for BodyReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        while self.cur.position() == self.cur.get_ref().len() as u64 {
            match self.it.next() {
                Some(Ok(chunk)) => self.cur = Cursor::new(chunk),
                Some(Err(e)) => return Err(io::Error::new(io::ErrorKind::Other, e)),
                None => break,
            }
        }

        self.cur.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.cur.consume(amt)
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

enum BodyWriterState {
    Buffering {
        request_size: u64,
        status: StatusCode,
        headers: HeaderMap,
        sender: oneshot::Sender<(hyper::Response<hyper::Body>, u64)>,
    },
    Writing {
        sender: body::Sender,
    },
    Done,
}

struct BodyWriter {
    state: BodyWriterState,
    buf: BytesMut,
}

impl Drop for BodyWriter {
    fn drop(&mut self) {
        if let BodyWriterState::Writing { sender } =
            mem::replace(&mut self.state, BodyWriterState::Done)
        {
            sender.abort();
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
                let buf = self.buf.take().freeze();

                let body = if buf.is_empty() {
                    hyper::Body::empty()
                } else {
                    hyper::Body::from(buf)
                };

                let mut response = hyper::Response::new(body);
                *response.status_mut() = status;
                *response.headers_mut() = headers;
                let _ = sender.send((response, request_size));
            }
            BodyWriterState::Writing { mut sender } => {
                let _ = self.send_chunk(&mut sender);
            }
            BodyWriterState::Done => {}
        }
    }

    fn send_chunk(&mut self, sender: &mut body::Sender) -> io::Result<()> {
        let buf = self.buf.take().freeze();
        if buf.is_empty() {
            return Ok(());
        }

        let f = BodySendFuture {
            sender,
            data: Some(Chunk::from(buf)),
        };
        f.wait()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
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
        let mut sender = match mem::replace(&mut self.state, BodyWriterState::Done) {
            BodyWriterState::Buffering {
                request_size,
                status,
                headers,
                sender,
            } => {
                let (body_sender, body) = hyper::Body::channel();

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

        let r = self.send_chunk(&mut sender);
        self.state = BodyWriterState::Writing { sender };
        r
    }
}

struct BodySendFuture<'a> {
    sender: &'a mut body::Sender,
    data: Option<Chunk>,
}

impl<'a> Future for BodySendFuture<'a> {
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
