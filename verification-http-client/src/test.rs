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

use futures::future::{self, Future};
use hyper::header::{HOST, RETRY_AFTER};
use hyper::server::conn::Http;
use hyper::service::Service;
use hyper::{self, Body, Request, Response, StatusCode, Version};
use parking_lot::Mutex;
use serde_json;
use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tokio::net::TcpStream;
use tokio::reactor::Handle;
use tokio::runtime::current_thread::Runtime;
use zipkin::{Endpoint, Tracer};

use config::{
    BasicCredentials, HostAndPort, HttpProxyConfig, ProxyConfig, SecurityConfig, ServiceConfig,
    ServiceDiscoveryConfig,
};
use {Agent, Client, UserAgent};

struct TestService<F>(Arc<Mutex<F>>);

impl<F> Service for TestService<F>
where
    F: FnMut(Request<Body>) -> Response<Body>,
{
    type ReqBody = Body;
    type ResBody = Body;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Response<Body>, Error = hyper::Error> + Send>;

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let mut f = self.0.lock();
        let f = &mut *f;
        Box::new(future::ok(f(req)))
    }
}

fn test_server<F>(requests: usize, callback: F) -> TestServer
where
    F: FnMut(Request<Body>) -> Response<Body> + 'static + Send,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = thread::spawn(move || {
        let mut runtime = Runtime::new().unwrap();
        let callback = Arc::new(Mutex::new(callback));

        for _ in 0..requests {
            let socket = listener.accept().unwrap().0;
            let f = future::lazy(|| Ok(TcpStream::from_std(socket, &Handle::current()).unwrap()))
                .and_then(|socket| {
                    Http::new()
                        .keep_alive(false)
                        .serve_connection(socket, TestService(callback.clone()))
                });
            runtime.block_on(f).unwrap();
        }
    });

    TestServer {
        handle: Some(handle),
        addr,
    }
}

struct TestServer {
    handle: Option<JoinHandle<()>>,
    addr: SocketAddr,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if !thread::panicking() {
            self.handle.take().unwrap().join().unwrap();
        }
    }
}

fn client(config: &str) -> Client {
    let config = serde_json::from_str(&config).unwrap();
    let agent = UserAgent::new(Agent::new("test", "1.0"));
    let tracer = Tracer::builder().build(Endpoint::builder().build());
    Client::new_static("service", agent, &tracer, &config).unwrap()
}

#[test]
fn google() {
    let discovery = ServiceDiscoveryConfig::builder()
        .service(
            "google",
            ServiceConfig::builder()
                .uris(vec!["https://www.google.com".parse().unwrap()])
                .build(),
        ).build();

    let agent = UserAgent::new(Agent::new("test", "1.0"));
    let tracer = Tracer::builder().build(Endpoint::builder().build());
    let client = Client::new_static("google", agent, &tracer, &discovery).unwrap();

    let response = client.get("/").send().unwrap();
    let mut body = vec![];
    response.raw_body().unwrap().read_to_end(&mut body).unwrap();
    println!("{}", String::from_utf8_lossy(&body));
}

#[test]
#[ignore]
fn google_http_proxy() {
    let discovery = ServiceDiscoveryConfig::builder()
        .service(
            "google",
            ServiceConfig::builder()
                .uris(vec!["http://www.google.com".parse().unwrap()])
                .proxy(ProxyConfig::Http(
                    HttpProxyConfig::builder()
                        .host_and_port(HostAndPort::new("localhost", 8080))
                        .credentials(Some(BasicCredentials::new("admin", "palantir")))
                        .build(),
                )).build(),
        ).build();

    let agent = UserAgent::new(Agent::new("test", "1.0"));
    let tracer = Tracer::builder().build(Endpoint::builder().build());
    let client = Client::new_static("google", agent, &tracer, &discovery).unwrap();

    let response = client.get("/").send().unwrap();
    let mut body = vec![];
    response.raw_body().unwrap().read_to_end(&mut body).unwrap();
    println!("{}", String::from_utf8_lossy(&body));
}

#[test]
#[ignore]
fn google_https_proxy() {
    let discovery = ServiceDiscoveryConfig::builder()
        .service(
            "google",
            ServiceConfig::builder()
                .uris(vec!["https://www.google.com".parse().unwrap()])
                .proxy(ProxyConfig::Http(
                    HttpProxyConfig::builder()
                        .host_and_port(HostAndPort::new("localhost", 8080))
                        .credentials(Some(BasicCredentials::new("admin", "palantir")))
                        .build(),
                )).security(
                    SecurityConfig::builder()
                        .ca_file(Some(
                            "/Users/sfackler/.mitmproxy/mitmproxy-ca-cert.pem".into(),
                        )).build(),
                ).build(),
        ).build();

    let agent = UserAgent::new(Agent::new("test", "1.0"));
    let tracer = Tracer::builder().build(Endpoint::builder().build());
    let client = Client::new_static("google", agent, &tracer, &discovery).unwrap();

    let response = client.get("/").send().unwrap();
    let mut body = vec![];
    response.raw_body().unwrap().read_to_end(&mut body).unwrap();
    println!("{}", String::from_utf8_lossy(&body));
}

#[test]
fn mesh_proxy() {
    let server = test_server(1, |req| {
        let host = req.headers().get(&HOST).unwrap();
        assert_eq!(host, "www.google.com:1234");
        assert_eq!(req.uri(), &"/foo/bar?fizz=buzz");

        Response::new(Body::empty())
    });

    let config = format!(
        r#"
        {{
            "services": {{
                "service": {{
                    "uris": [
                        "http://www.google.com:1234"
                    ],
                    "proxy": {{
                        "type": "mesh",
                        "host-and-port": "127.0.0.1:{}"
                    }}
                }}
            }}
        }}
        "#,
        server.addr.port()
    );
    let client = client(&config);

    client.get("/foo/bar").param("fizz", "buzz").send().unwrap();
}

#[test]
fn failover_after_503() {
    static SERVER1_HIT: AtomicBool = AtomicBool::new(false);

    let server1 = test_server(1, |_| {
        SERVER1_HIT.store(true, Ordering::SeqCst);
        Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Body::empty())
            .unwrap()
    });
    let server2 = test_server(1, |_| Response::new(Body::empty()));

    let config = format!(
        r#"
        {{
            "services": {{
                "service": {{
                    "uris": [
                        "http://localhost:{}",
                        "http://localhost:{}"
                    ]
                }}
            }}
        }}
        "#,
        server1.addr.port(),
        server2.addr.port()
    );
    let client = client(&config);

    let response = client.get("/").send().unwrap();
    assert!(SERVER1_HIT.load(Ordering::SeqCst));
    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn retry_after_overrides() {
    let mut hit = false;
    let server = test_server(2, move |_| {
        if !hit {
            hit = true;
            Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header(RETRY_AFTER, "1")
                .body(Body::empty())
                .unwrap()
        } else {
            Response::new(Body::empty())
        }
    });

    let config = format!(
        r#"
        {{
            "services": {{
                "service": {{
                    "uris": [
                        "http://localhost:{}"
                    ],
                    "backoff-slot-size": "1h"
                }}
            }}
        }}
        "#,
        server.addr.port(),
    );
    let client = client(&config);

    let response = client.get("/").send().unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn assume_http2() {
    let server = test_server(1, |request| {
        assert_eq!(request.version(), Version::HTTP_2);
        Response::new(Body::empty())
    });

    let config = format!(
        r#"
        {{
            "services": {{
                "service": {{
                    "uris": ["http://localhost:{}"],
                    "experimental-assume-http2": true
                }}
            }}
        }}
        "#,
        server.addr.port()
    );
    let client = client(&config);

    let response = client.get("/").send().unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
