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

use crate::config::HostAndPort;
use crate::r#async::socket::SocketConnector;
use crate::ProxyAuthorization;
use conjure_verification_http::headers;
use http_body_util::Empty;
use hyper::body::Bytes;
use hyper::client::conn::http1 as client_conn;
use hyper::{Method, Request, Uri, Version};
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::TokioIo;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpStream;
use tower_service::Service;

/// A TCP stream that carries hyper-util `Connection` metadata (e.g. whether
/// it went through a proxy). It implements tokio's `AsyncRead`/`AsyncWrite`
/// directly and is wrapped in `TokioIo` by hyper-openssl/hyper-util.
pub struct ConnStream {
    stream: TcpStream,
    proxied: bool,
}

impl Connection for ConnStream {
    fn connected(&self) -> Connected {
        Connected::new().proxy(self.proxied)
    }
}

impl hyper::rt::Read for ConnStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut TokioIo::new(&mut self.stream)).poll_read(cx, buf)
    }
}

impl hyper::rt::Write for ConnStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut TokioIo::new(&mut self.stream)).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut TokioIo::new(&mut self.stream)).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut TokioIo::new(&mut self.stream)).poll_shutdown(cx)
    }
}

#[derive(Clone)]
pub struct ProxyConnectorConfig {
    pub addr: HostAndPort,
    pub credentials: Option<ProxyAuthorization>,
}

#[derive(Clone)]
pub struct ProxyConnector {
    connector: SocketConnector,
    proxy: Option<ProxyConnectorConfig>,
}

impl ProxyConnector {
    pub fn new(connector: SocketConnector, proxy: Option<ProxyConnectorConfig>) -> ProxyConnector {
        ProxyConnector { connector, proxy }
    }
}

type BoxError = Box<dyn Error + Sync + Send>;

impl Service<Uri> for ProxyConnector {
    type Response = ConnStream;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, dst: Uri) -> Self::Future {
        let connector = self.connector;
        let proxy = self.proxy.clone();
        Box::pin(async move {
            let scheme = dst.scheme_str().unwrap_or("http");
            let default_port = if scheme == "https" { 443 } else { 80 };
            let host = dst.host().ok_or("missing host in URI")?;
            let port = dst.port_u16().unwrap_or(default_port);

            match (&proxy, scheme) {
                (Some(p), "https") => {
                    // CONNECT tunnel through the proxy
                    let stream = connector.connect(p.addr.host(), p.addr.port()).await?;
                    let io = TokioIo::new(stream);

                    let (mut sender, conn) = client_conn::handshake(io).await?;

                    let connect_uri = format!("{}:{}", host, port).parse::<Uri>().unwrap();
                    let mut request = Request::new(Empty::<Bytes>::new());
                    *request.method_mut() = Method::CONNECT;
                    *request.uri_mut() = connect_uri;
                    *request.version_mut() = Version::HTTP_11;
                    headers::set_host(request.headers_mut(), host, Some(port));
                    if let Some((ref username, ref password)) = p.credentials {
                        headers::set_proxy_authorization_basic(
                            request.headers_mut(),
                            username,
                            password,
                        );
                    }

                    let resp = sender.send_request(request).await?;
                    if !resp.status().is_success() {
                        return Err(format!("got status {} from HTTPS proxy", resp.status()).into());
                    }

                    // The CONNECT request is complete; take the underlying
                    // stream back out of the connection.
                    let parts = conn.without_shutdown().await?;
                    let stream = parts.io.into_inner();
                    Ok(ConnStream {
                        stream,
                        proxied: false,
                    })
                }
                (Some(p), _) => {
                    let stream = connector.connect(p.addr.host(), p.addr.port()).await?;
                    Ok(ConnStream {
                        stream,
                        proxied: true,
                    })
                }
                (None, _) => {
                    let stream = connector.connect(host, port).await?;
                    Ok(ConnStream {
                        stream,
                        proxied: false,
                    })
                }
            }
        })
    }
}
