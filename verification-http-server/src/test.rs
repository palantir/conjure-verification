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

use crate::router::{Binder, Router};
use crate::server;
use bytes::Bytes;
use conjure_verification_error::{Error, Result};
use conjure_verification_http::headers;
use conjure_verification_http::resource::{Resource, Route};
use conjure_verification_http::response::{Body, Response, WriteBody};
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, HOST, TRANSFER_ENCODING};
use http::response::Parts;
use http::StatusCode;
use http_body_util::{BodyExt, Empty};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

enum Failure {
    Error,
    Panic,
}

struct TestBody {
    len: usize,
    failure: Option<Failure>,
}

impl WriteBody for TestBody {
    fn write_body(&mut self, w: &mut dyn Write) -> Result<()> {
        w.write_all(&vec![b'a'; self.len])
            .map_err(Error::internal_safe)?;
        match self.failure {
            None => Ok(()),
            Some(Failure::Error) => Err(Error::internal_safe("body failed")),
            Some(Failure::Panic) => panic!("body panicked"),
        }
    }
}

fn streaming(len: usize, failure: Option<Failure>) -> Response {
    let mut response = Response::new(StatusCode::OK);
    headers::set_content_type(&mut response.headers, &mime::APPLICATION_OCTET_STREAM);
    response.body = Body::Streaming(Box::new(TestBody { len, failure }));
    response
}

struct TestResource;

impl Resource for TestResource {
    const BASE_PATH: &'static str = "";

    fn register<R>(router: &mut R)
    where
        R: Route<Self>,
    {
        router.get("/json", "", |_, _| Ok("hello"));
        router.get("/stream/small", "", |_, _| Ok(streaming(5, None)));
        router.get("/stream/error-early", "", |_, _| {
            Ok(streaming(10, Some(Failure::Error)))
        });
        router.get("/stream/panic-early", "", |_, _| {
            Ok(streaming(10, Some(Failure::Panic)))
        });
        router.get("/stream/error-late", "", |_, _| {
            Ok(streaming(5000, Some(Failure::Error)))
        });
        router.get("/stream/panic-late", "", |_, _| {
            Ok(streaming(5000, Some(Failure::Panic)))
        });
    }
}

async fn start_server() -> SocketAddr {
    let mut builder = Router::builder();
    TestResource::register(&mut Binder::new(Arc::new(TestResource), &mut builder, ""));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(server::serve(listener, Arc::new(builder.build())));
    addr
}

type BodyResult = std::result::Result<Bytes, hyper::Error>;

async fn send_http1(addr: SocketAddr, path: &str) -> hyper::Result<http::Response<Incoming>> {
    let stream = TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);
    let request = http::Request::get(path)
        .header(HOST, "localhost")
        .body(Empty::<Bytes>::new())
        .unwrap();
    sender.send_request(request).await
}

async fn get_http1(addr: SocketAddr, path: &str) -> (Parts, BodyResult) {
    let (parts, body) = send_http1(addr, path).await.unwrap().into_parts();
    (parts, body.collect().await.map(|b| b.to_bytes()))
}

/// Asserts that the response is aborted rather than completing successfully. Depending on how much the server wrote
/// before aborting, the client sees either no response at all or a truncated body.
async fn assert_aborted(addr: SocketAddr, path: &str) {
    if let Ok(response) = send_http1(addr, path).await {
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.map(|b| b.to_bytes());
        assert!(body.is_err(), "body should be truncated, got {:?}", body);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_http2_prior_knowledge() {
    let addr = start_server().await;

    let stream = TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(stream))
            .await
            .unwrap();
    tokio::spawn(conn);
    let request = http::Request::get(format!("http://{}/json", addr))
        .body(Empty::<Bytes>::new())
        .unwrap();
    let response = sender.send_request(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body, "\"hello\"");
}

#[tokio::test(flavor = "multi_thread")]
async fn small_streamed_body_is_sent_with_content_length() {
    let addr = start_server().await;

    let (parts, body) = get_http1(addr, "/stream/small").await;

    assert_eq!(parts.status, StatusCode::OK);
    assert_eq!(parts.headers.get(CONTENT_LENGTH).unwrap(), "5");
    assert!(parts.headers.get(TRANSFER_ENCODING).is_none());
    assert_eq!(body.unwrap(), "aaaaa");
}

#[tokio::test(flavor = "multi_thread")]
async fn streamed_body_error_before_flush_returns_error_response() {
    let addr = start_server().await;

    let (parts, body) = get_http1(addr, "/stream/error-early").await;

    assert_eq!(parts.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(parts.headers.get(CONTENT_TYPE).unwrap(), "application/json");
    assert!(body.is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn streamed_body_panic_before_flush_returns_500() {
    let addr = start_server().await;

    let (parts, _) = get_http1(addr, "/stream/panic-early").await;

    assert_eq!(parts.status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test(flavor = "multi_thread")]
async fn streamed_body_error_after_flush_aborts_response() {
    let addr = start_server().await;

    assert_aborted(addr, "/stream/error-late").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn streamed_body_panic_after_flush_aborts_response() {
    let addr = start_server().await;

    assert_aborted(addr, "/stream/panic-late").await;
}
