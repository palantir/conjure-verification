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

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::{HOST, RETRY_AFTER};
use hyper::{Request, Response, StatusCode, Version};
use hyper_util::rt::TokioIo;
use openssl::ssl::{self, AlpnError, SslAcceptor, SslAcceptorBuilder, SslFiletype, SslMethod};
use parking_lot::Mutex;
use serde_json;
use std::convert::Infallible;
use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tokio::runtime::Runtime;

use tokio_openssl::SslStream;

type TestBody = Full<Bytes>;
type TestRequest = Request<Incoming>;
type TestResponse = Response<TestBody>;

fn empty_body() -> TestBody {
    Full::new(Bytes::new())
}

use crate::config::{
    BasicCredentials, HostAndPort, HttpProxyConfig, ProxyConfig, SecurityConfig, ServiceConfig,
    ServiceDiscoveryConfig,
};
use crate::{Agent, Client, UserAgent};

struct TestService<F>(Arc<Mutex<F>>);

impl<F> TestService<F>
where
    F: FnMut(TestRequest) -> TestResponse,
{
    fn call(&self, req: TestRequest) -> TestResponse {
        let mut f = self.0.lock();
        let f = &mut *f;
        f(req)
    }
}

fn test_tls_server<F, G>(requests: usize, acceptor_callback: F, callback: G) -> TestTlsServer
where
    F: FnOnce(&mut SslAcceptorBuilder),
    G: FnMut(TestRequest) -> TestResponse + 'static + Send,
{
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test");
    let key_file = test_dir.join("key.pem");
    let cert_file = test_dir.join("cert.cer");

    let mut acceptor = SslAcceptor::mozilla_modern(SslMethod::tls()).unwrap();
    acceptor
        .set_private_key_file(&key_file, SslFiletype::PEM)
        .unwrap();
    acceptor.set_certificate_chain_file(&cert_file).unwrap();
    acceptor_callback(&mut acceptor);
    let acceptor = acceptor.build();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = thread::spawn(move || {
        let runtime = Runtime::new().unwrap();
        let callback = Arc::new(Mutex::new(callback));

        listener.set_nonblocking(true).unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            for _ in 0..requests {
                let (stream, _) = listener.accept().await.unwrap();
                let ssl_config = openssl::ssl::Ssl::new(acceptor.context()).unwrap();
                let mut ssl = SslStream::new(ssl_config, stream).unwrap();
                std::pin::Pin::new(&mut ssl).accept().await.unwrap();
                let h2 = ssl.ssl().selected_alpn_protocol() == Some(b"h2");
                let svc = TestService(callback.clone());
                let svc = hyper::service::service_fn(move |req| {
                    let svc = TestService(svc.0.clone());
                    async move { Ok::<_, Infallible>(svc.call(req)) }
                });
                if h2 {
                    hyper::server::conn::http2::Builder::new(hyper_util::rt::TokioExecutor::new())
                        .serve_connection(TokioIo::new(ssl), svc)
                        .await
                        .unwrap();
                } else {
                    let mut builder = hyper::server::conn::http1::Builder::new();
                    builder.keep_alive(false);
                    builder
                        .serve_connection(TokioIo::new(ssl), svc)
                        .await
                        .unwrap();
                }
            }
        });
    });

    TestTlsServer {
        handle: Some(handle),
        addr,
        cert_file,
    }
}

fn test_server<F>(requests: usize, callback: F) -> TestServer
where
    F: FnMut(TestRequest) -> TestResponse + 'static + Send,
{
    test_server_impl(requests, false, callback)
}

fn test_server_h2<F>(requests: usize, callback: F) -> TestServer
where
    F: FnMut(TestRequest) -> TestResponse + 'static + Send,
{
    test_server_impl(requests, true, callback)
}

fn test_server_impl<F>(requests: usize, h2: bool, callback: F) -> TestServer
where
    F: FnMut(TestRequest) -> TestResponse + 'static + Send,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = thread::spawn(move || {
        let runtime = Runtime::new().unwrap();
        let callback = Arc::new(Mutex::new(callback));

        listener.set_nonblocking(true).unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            for _ in 0..requests {
                let (stream, _) = listener.accept().await.unwrap();
                let svc = TestService(callback.clone());
                if h2 {
                    hyper::server::conn::http2::Builder::new(hyper_util::rt::TokioExecutor::new())
                        .serve_connection(
                            TokioIo::new(stream),
                            hyper::service::service_fn(move |req| {
                                let svc = TestService(svc.0.clone());
                                async move { Ok::<_, Infallible>(svc.call(req)) }
                            }),
                        )
                        .await
                        .unwrap();
                } else {
                    let mut builder = hyper::server::conn::http1::Builder::new();
                    builder.keep_alive(false);
                    builder
                        .serve_connection(
                            TokioIo::new(stream),
                            hyper::service::service_fn(move |req| {
                                let svc = TestService(svc.0.clone());
                                async move { Ok::<_, Infallible>(svc.call(req)) }
                            }),
                        )
                        .await
                        .unwrap();
                }
            }
        });
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

struct TestTlsServer {
    handle: Option<JoinHandle<()>>,
    addr: SocketAddr,
    cert_file: PathBuf,
}

impl Drop for TestTlsServer {
    fn drop(&mut self) {
        if !thread::panicking() {
            self.handle.take().unwrap().join().unwrap();
        }
    }
}

fn client(config: &str) -> Client {
    let config = serde_json::from_str(&config).unwrap();
    let agent = UserAgent::new(Agent::new("test", "1.0"));
    Client::new_static("service", agent, &config).unwrap()
}

#[test]
fn read_timeout_is_applied_to_http_requests() {
    use std::io::Write;
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let mut stream = listener.accept().unwrap().0;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert!(stream.read(&mut [0; 4096]).unwrap() > 0);
        thread::sleep(Duration::from_millis(500));
        // The client should have timed out and closed the socket already.
        let _ =
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    });
    let config = format!(
        r#"{{"services": {{"service": {{
            "uris": ["http://127.0.0.1:{port}"],
            "read-timeout": "50ms",
            "max-num-retries": 0
        }}}}}}"#,
    );
    let response = client(&config).get("/").send();
    server.join().unwrap();
    assert!(response.is_err(), "request ignored the read timeout");
}

#[test]
fn google() {
    let discovery = ServiceDiscoveryConfig::builder()
        .service(
            "google",
            ServiceConfig::builder()
                .uris(vec!["https://www.google.com".parse().unwrap()])
                .build(),
        )
        .build();

    let agent = UserAgent::new(Agent::new("test", "1.0"));
    let client = Client::new_static("google", agent, &discovery).unwrap();

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
                ))
                .build(),
        )
        .build();

    let agent = UserAgent::new(Agent::new("test", "1.0"));
    let client = Client::new_static("google", agent, &discovery).unwrap();

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
                ))
                .security(
                    SecurityConfig::builder()
                        .ca_file(Some(
                            "/Users/sfackler/.mitmproxy/mitmproxy-ca-cert.pem".into(),
                        ))
                        .build(),
                )
                .build(),
        )
        .build();

    let agent = UserAgent::new(Agent::new("test", "1.0"));
    let client = Client::new_static("google", agent, &discovery).unwrap();

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

        Response::new(empty_body())
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
            .body(empty_body())
            .unwrap()
    });
    let server2 = test_server(1, |_| Response::new(empty_body()));

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
                .body(empty_body())
                .unwrap()
        } else {
            Response::new(empty_body())
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
    let server = test_server_h2(1, |request| {
        assert_eq!(request.version(), Version::HTTP_2);
        Response::new(empty_body())
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

#[test]
fn assume_http2_tls() {
    let server = test_tls_server(
        1,
        |ssl| {
            ssl.set_alpn_select_callback(|_, client| {
                ssl::select_next_proto(b"\x02h2", client).ok_or(AlpnError::ALERT_FATAL)
            });
        },
        |request| {
            assert_eq!(request.version(), Version::HTTP_2);
            Response::new(empty_body())
        },
    );

    let config = format!(
        r#"
        {{
            "services": {{
                "service": {{
                    "uris": ["https://localhost:{}"],
                    "experimental-assume-http2": true,
                    "security": {{
                        "ca-file": "{}"
                    }}
                }}
            }}
        }}
        "#,
        server.addr.port(),
        server.cert_file.display()
    );
    let client = client(&config);

    let response = client.get("/").send().unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// Starts a server which keeps connections alive and counts the connections it accepts.
fn counting_server() -> (SocketAddr, Arc<std::sync::atomic::AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let counter = connections.clone();
    thread::spawn(move || {
        let runtime = Runtime::new().unwrap();
        listener.set_nonblocking(true).unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(hyper::server::conn::http1::Builder::new().serve_connection(
                    TokioIo::new(stream),
                    hyper::service::service_fn(|_| async {
                        Ok::<_, Infallible>(Response::new(empty_body()))
                    }),
                ));
            }
        });
    });

    (addr, connections)
}

fn connections_for_three_requests(keep_alive: bool) -> usize {
    let (addr, connections) = counting_server();
    let config = format!(
        r#"{{"services": {{"service": {{
            "uris": ["http://127.0.0.1:{}"],
            "keep-alive": {}
        }}}}}}"#,
        addr.port(),
        keep_alive
    );
    let client = client(&config);
    for _ in 0..3 {
        client.get("/").send().unwrap();
    }
    connections.load(Ordering::SeqCst)
}

#[test]
fn keep_alive_reuses_connections() {
    assert_eq!(connections_for_three_requests(true), 1);
}

#[test]
fn disabling_keep_alive_opens_a_connection_per_request() {
    assert_eq!(connections_for_three_requests(false), 3);
}

#[test]
fn truncated_response_body_is_an_error() {
    use std::io::Write;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let mut stream = listener.accept().unwrap().0;
        assert!(stream.read(&mut [0; 4096]).unwrap() > 0);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nonly 11 bytes")
            .unwrap();
    });
    let config =
        format!(r#"{{"services": {{"service": {{"uris": ["http://127.0.0.1:{port}"]}}}}}}"#);

    let response = client(&config).get("/").send().unwrap();
    server.join().unwrap();
    let mut body = vec![];
    let result = response.raw_body().unwrap().read_to_end(&mut body);

    assert!(result.is_err(), "read {:?}", String::from_utf8_lossy(&body));
}

fn connect_error(uri: &str) -> String {
    let config =
        format!(r#"{{"services": {{"service": {{"uris": ["{uri}"], "max-num-retries": 0}}}}}}"#);
    match client(&config).get("/").send() {
        Ok(response) => panic!("expected an error, got {}", response.status()),
        Err(e) => e.cause().to_string(),
    }
}

#[test]
fn connect_error_includes_cause() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let error = connect_error(&format!("http://127.0.0.1:{}", port));

    assert!(
        error.to_lowercase().contains("connection refused"),
        "{}",
        error
    );
}

#[test]
fn unknown_uri_scheme_is_rejected() {
    let (addr, connections) = counting_server();

    let error = connect_error(&format!("ftp://127.0.0.1:{}", addr.port()));

    assert!(error.contains("invalid URI scheme"), "{}", error);
    assert_eq!(connections.load(Ordering::SeqCst), 0);
}

/// A proxy which accepts one CONNECT request, sends its request head to the returned receiver, and then tunnels the
/// connection to `target`.
fn connect_proxy(target: SocketAddr) -> (SocketAddr, std::sync::mpsc::Receiver<String>) {
    use std::io::Write;
    use std::net::{Shutdown, TcpStream};

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut client = listener.accept().unwrap().0;
        // read a byte at a time so we don't consume anything after the request head
        let mut head = vec![];
        while !head.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            client.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        tx.send(String::from_utf8(head).unwrap()).unwrap();

        let mut server = TcpStream::connect(target).unwrap();
        client
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .unwrap();
        let mut client_read = client.try_clone().unwrap();
        let mut server_write = server.try_clone().unwrap();
        thread::spawn(move || {
            let _ = std::io::copy(&mut client_read, &mut server_write);
            let _ = server_write.shutdown(Shutdown::Write);
        });
        let _ = std::io::copy(&mut server, &mut client);
        let _ = client.shutdown(Shutdown::Write);
    });
    (addr, rx)
}

#[test]
fn https_proxy_tunnels_with_connect() {
    let server = test_tls_server(
        1,
        |_| {},
        |request| {
            assert_eq!(request.uri(), "/foo");
            Response::new(empty_body())
        },
    );
    let (proxy_addr, proxy_requests) = connect_proxy(server.addr);

    let config = format!(
        r#"{{"services": {{"service": {{
            "uris": ["https://localhost:{}"],
            "security": {{"ca-file": "{}"}},
            "proxy": {{
                "type": "http",
                "host-and-port": "127.0.0.1:{}",
                "credentials": {{"username": "admin", "password": "palantir"}}
            }},
            "max-num-retries": 0
        }}}}}}"#,
        server.addr.port(),
        server.cert_file.display(),
        proxy_addr.port()
    );
    let response = client(&config).get("/foo").send().unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let head = proxy_requests.recv().unwrap().to_lowercase();
    let connect = format!("localhost:{}", server.addr.port());
    assert!(
        head.starts_with(&format!("connect {} http/1.1\r\n", connect)),
        "{}",
        head
    );
    assert!(
        head.contains(&format!("\r\nhost: {}\r\n", connect)),
        "{}",
        head
    );
    assert!(
        head.contains("\r\nproxy-authorization: basic ywrtaw46cgfsyw50axi=\r\n"),
        "{}",
        head
    );
}

#[test]
fn trusts_os_root_certificates() {
    // OpenSSL is statically linked, so it has to be pointed at the OS's root certificates. Run this test with
    // SSL_CERT_FILE and SSL_CERT_DIR unset to check that.
    let connector = crate::ssl_connector().unwrap().build();
    let certificates = connector.context().cert_store().all_certificates();
    assert!(!certificates.is_empty());
}
