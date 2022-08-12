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

use config::HostAndPort;
use futures::{Async, Future, Poll};
use hyper::client::conn::{self, Connection, Handshake, ResponseFuture};
use hyper::client::connect::{Connect, Connected, Destination};
use hyper::{Body, Method, Request, Version};
use state_machine_future::RentToOwn;
use std::error::Error;
use tokio::net::TcpStream;
use tokio_io_timeout::TimeoutStream;
use typed_headers::{HeaderMapExt, Host, ProxyAuthorization};

use async::socket::{SocketConnectFuture, SocketConnector};

#[derive(Clone)]
pub struct ProxyConnectorConfig {
    pub addr: HostAndPort,
    pub credentials: Option<ProxyAuthorization>,
}

pub struct ProxyConnector {
    connector: SocketConnector,
    proxy: Option<ProxyConnectorConfig>,
}

impl ProxyConnector {
    pub fn new(connector: SocketConnector, proxy: Option<ProxyConnectorConfig>) -> ProxyConnector {
        ProxyConnector { connector, proxy }
    }
}

impl Connect for ProxyConnector {
    type Transport = TimeoutStream<TcpStream>;
    type Error = Box<Error + Sync + Send>;
    type Future = ProxyConnectFuture;

    fn connect(&self, dst: Destination) -> ProxyConnectFuture {
        ProxyConnect::start(self.connector, self.proxy.clone(), dst)
    }
}

#[derive(StateMachineFuture)]
pub enum ProxyConnect {
    #[state_machine_future(
        start,
        transitions(ConnectingDirect, ConnectingHttpProxy, ConnectingHttpsProxy)
    )]
    Start {
        connector: SocketConnector,
        proxy: Option<ProxyConnectorConfig>,
        dst: Destination,
    },
    #[state_machine_future(transitions(Finished))]
    ConnectingDirect { conn: SocketConnectFuture },
    #[state_machine_future(transitions(Finished))]
    ConnectingHttpProxy { conn: SocketConnectFuture },
    #[state_machine_future(transitions(TunnelHandshaking))]
    ConnectingHttpsProxy {
        conn: SocketConnectFuture,
        proxy: ProxyConnectorConfig,
        dst: Destination,
    },
    #[state_machine_future(transitions(TunnelConnecting))]
    TunnelHandshaking {
        conn: Handshake<TimeoutStream<TcpStream>, Body>,
        proxy: ProxyConnectorConfig,
        dst: Destination,
    },
    #[state_machine_future(transitions(Finished))]
    TunnelConnecting {
        resp: ResponseFuture,
        conn: Connection<TimeoutStream<TcpStream>, Body>,
    },
    #[state_machine_future(ready)]
    Finished((TimeoutStream<TcpStream>, Connected)),
    #[state_machine_future(error)]
    Failed(Box<Error + Sync + Send>),
}

impl PollProxyConnect for ProxyConnect {
    fn poll_start<'a>(
        start: &'a mut RentToOwn<'a, Start>,
    ) -> Poll<AfterStart, Box<Error + Sync + Send>> {
        let start = start.take();

        let default_port = match start.dst.scheme() {
            "http" => 80,
            "https" => 443,
            _ => return Err("invalid URI scheme".into()),
        };

        let after = match (start.proxy, start.dst.scheme()) {
            (Some(proxy), "https") => ConnectingHttpsProxy {
                conn: start
                    .connector
                    .connect(proxy.addr.host(), proxy.addr.port()),
                proxy,
                dst: start.dst,
            }
            .into(),
            (Some(proxy), _) => ConnectingHttpProxy {
                conn: start
                    .connector
                    .connect(proxy.addr.host(), proxy.addr.port()),
            }
            .into(),
            (None, _) => {
                let port = start.dst.port().unwrap_or(default_port);
                ConnectingDirect {
                    conn: start.connector.connect(start.dst.host(), port),
                }
                .into()
            }
        };

        Ok(Async::Ready(after))
    }

    fn poll_connecting_direct<'a>(
        state: &'a mut RentToOwn<'a, ConnectingDirect>,
    ) -> Poll<AfterConnectingDirect, Box<Error + Sync + Send>> {
        let stream = try_ready!(state.conn.poll());
        let connected = Connected::new();

        Ok(Async::Ready(Finished((stream, connected)).into()))
    }

    fn poll_connecting_http_proxy<'a>(
        state: &'a mut RentToOwn<'a, ConnectingHttpProxy>,
    ) -> Poll<AfterConnectingHttpProxy, Box<Error + Sync + Send>> {
        let stream = try_ready!(state.conn.poll());
        let connected = Connected::new().proxy(true);

        Ok(Async::Ready(Finished((stream, connected)).into()))
    }

    fn poll_connecting_https_proxy<'a>(
        state: &'a mut RentToOwn<'a, ConnectingHttpsProxy>,
    ) -> Poll<AfterConnectingHttpsProxy, Box<Error + Sync + Send>> {
        let stream = try_ready!(state.conn.poll());
        let state = state.take();

        Ok(Async::Ready(
            TunnelHandshaking {
                conn: conn::handshake(stream),
                proxy: state.proxy,
                dst: state.dst,
            }
            .into(),
        ))
    }

    fn poll_tunnel_handshaking<'a>(
        state: &'a mut RentToOwn<'a, TunnelHandshaking>,
    ) -> Poll<AfterTunnelHandshaking, Box<Error + Sync + Send>> {
        let (mut sender, conn) = try_ready!(state.conn.poll());
        let state = state.take();

        let dst = format!("{}:{}", state.dst.host(), state.dst.port().unwrap_or(443))
            .parse()
            .unwrap();

        let host = Host::new(state.proxy.addr.host(), Some(state.proxy.addr.port()))?;

        let mut request = Request::new(Body::empty());
        *request.method_mut() = Method::CONNECT;
        *request.uri_mut() = dst;
        *request.version_mut() = Version::HTTP_11;
        request.headers_mut().typed_insert(&host);
        if let Some(ref auth) = state.proxy.credentials {
            request.headers_mut().typed_insert(auth);
        }

        let resp = sender.send_request(request);

        Ok(Async::Ready(TunnelConnecting { conn, resp }.into()))
    }

    fn poll_tunnel_connecting<'a>(
        state: &'a mut RentToOwn<'a, TunnelConnecting>,
    ) -> Poll<AfterTunnelConnecting, Box<Error + Sync + Send>> {
        state.conn.poll_without_shutdown()?;
        let resp = try_ready!(state.resp.poll());
        let state = state.take();

        if !resp.status().is_success() {
            return Err(format!("got status {} from HTTPS proxy", resp.status()).into());
        }

        let conn = state.conn.into_parts().io;
        let connected = Connected::new();
        Ok(Async::Ready(Finished((conn, connected)).into()))
    }
}
