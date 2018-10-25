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

extern crate base64;
extern crate bytes;
extern crate conjure_verification_error;
extern crate conjure_verification_http_client_config;
extern crate crossbeam;
extern crate flate2;
extern crate http_zipkin;
extern crate hyper;
extern crate hyper_openssl;
extern crate mime;
extern crate openssl;
extern crate parking_lot;
extern crate rand;
extern crate regex;
extern crate serde;
extern crate serde_cbor;
extern crate serde_json;
extern crate serde_urlencoded;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_io_timeout;
extern crate tokio_threadpool;
extern crate typed_headers;
extern crate url;
extern crate zipkin;

#[macro_use]
extern crate futures;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate state_machine_future;
#[macro_use]
extern crate log;

#[cfg(test)]
extern crate tokio_openssl;

use config::{HostAndPort, ProxyConfig, ServiceDiscoveryConfig};
use crossbeam::sync::ArcCell;
use errors::{Error, Result, SerializableError};
use hyper::header::HeaderValue;
use hyper::{Method, StatusCode};
use hyper_openssl::HttpsConnector;
use mime::Mime;
use openssl::error::ErrorStack;
use openssl::ssl::{SslConnector, SslMethod};
use std::error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::{self, Runtime};
use typed_headers::{Credentials, ProxyAuthorization};
use zipkin::Tracer;

use async::alpn::AlpnConnector;
use async::custom_error::CustomErrorConnector;
use async::proxy::{ProxyConnector, ProxyConnectorConfig};
use async::socket::{SocketConnector, Timeouts};
pub use body::*;
use node_selector::NodeSelector;
pub use reloadable::*;
pub use request::*;
pub use response::*;
pub use user_agent::*;

#[doc(inline)]
pub use hyper::header;

pub mod config {
    pub use conjure_verification_http_client_config::*;
}

mod errors {
    pub use conjure_verification_error::*;
}

pub mod async;
pub mod backoff;
pub mod body;
pub mod node_selector;
pub mod reloadable;
pub mod request;
pub mod response;
pub mod user_agent;

#[cfg(test)]
mod test;

lazy_static! {
    static ref RUNTIME: Runtime = {
        let mut pool = tokio_threadpool::Builder::new();
        // we use blocking for DNS lookup so we don't need/want a ton of parallelism available
        pool.max_blocking(2)
            .keep_alive(Some(Duration::from_secs(30)))
            .name_prefix("chatter-");

        #[allow(deprecated)]
        runtime::Builder::new()
            .threadpool_builder(pool)
            .build()
            .unwrap()
    };
    static ref APPLICATION_CBOR: Mime = "application/cbor".parse().unwrap();
}

#[derive(Debug)]
pub struct RemoteError {
    status: StatusCode,
    error: Option<SerializableError>,
}

impl fmt::Display for RemoteError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self.error() {
            Some(ref error) => write!(
                fmt,
                "remote error: {} ({}) with instance ID {}",
                error.code(),
                error.name(),
                error.id()
            ),
            None => write!(fmt, "remote error: {}", self.status),
        }
    }
}

impl error::Error for RemoteError {
    fn description(&self) -> &str {
        "server error"
    }
}

impl RemoteError {
    pub fn status(&self) -> &StatusCode {
        &self.status
    }

    pub fn error(&self) -> Option<&SerializableError> {
        self.error.as_ref()
    }
}

fn extract_config(service: &str, discovery_config: &ServiceDiscoveryConfig) -> Result<ClientState> {
    let service_config = match discovery_config.service(service) {
        Some(service_config) => service_config,
        None => {
            return Err(Error::internal_safe("service not found in configuration")
                .with_safe_param("service", service))
        }
    };

    let nodes = NodeSelector::new(service_config.uris());

    let mut ssl = SslConnector::builder(SslMethod::tls()).map_err(Error::internal_safe)?;

    if let Some(ref ca_file) = service_config.security().ca_file() {
        ssl.set_ca_file(ca_file).map_err(Error::internal_safe)?;
        // https://github.com/openssl/openssl/issues/6851
        ErrorStack::get();
    }

    if service_config.experimental_assume_http2() {
        ssl.set_alpn_protos(b"\x02h2")
            .map_err(Error::internal_safe)?;
    }

    let (proxy_state, proxy) = match *service_config.proxy() {
        ProxyConfig::Http(ref config) => {
            let credentials = match config.credentials() {
                Some(credentials) => {
                    let creds = Credentials::basic(credentials.username(), credentials.password())
                        .map_err(Error::internal_safe)?;
                    Some(ProxyAuthorization(creds))
                }
                None => None,
            };

            (
                Some(ProxyState::Http {
                    credentials: credentials.clone(),
                }),
                Some(ProxyConnectorConfig {
                    addr: config.host_and_port().clone(),
                    credentials,
                }),
            )
        }
        ProxyConfig::Mesh(ref config) => (
            Some(ProxyState::Mesh {
                host: config.host_and_port().clone(),
            }),
            None,
        ),
        ProxyConfig::Direct => (None, None),
        _ => return Err(Error::internal_safe("unknown proxy type")),
    };

    let timeouts = Timeouts {
        connect: service_config.connect_timeout(),
        read: service_config.read_timeout(),
        write: service_config.write_timeout(),
    };
    let connector = SocketConnector(timeouts);
    let connector = ProxyConnector::new(connector, proxy);
    let connector = HttpsConnector::with_connector(connector, ssl).map_err(Error::internal_safe)?;
    let connector = AlpnConnector::new(connector, service_config.experimental_assume_http2());
    let connector = CustomErrorConnector(connector);

    let client = hyper::Client::builder()
        .keep_alive(service_config.keep_alive())
        .http2_only(service_config.experimental_assume_http2())
        .http1_writev(false)
        .executor(RUNTIME.executor())
        .build(connector);

    Ok(ClientState {
        client,
        nodes,
        max_num_retries: service_config.max_num_retries(),
        backoff_slot_size: service_config.backoff_slot_size(),
        proxy: proxy_state,
    })
}

struct ClientState {
    client: hyper::Client<CustomErrorConnector>,
    nodes: NodeSelector,
    max_num_retries: u32,
    backoff_slot_size: Duration,
    proxy: Option<ProxyState>,
}

enum ProxyState {
    Http {
        credentials: Option<ProxyAuthorization>,
    },
    Mesh {
        host: HostAndPort,
    },
}

/// An HTTP client to a remote service.
pub struct Client {
    service: String,
    user_agent: HeaderValue,
    tracer: Tracer,
    reload: Option<Reloadable<ServiceDiscoveryConfig>>,
    state: ArcCell<ClientState>,
}

impl Client {
    pub fn new(
        service: &str,
        user_agent: UserAgent,
        tracer: &Tracer,
        config: Reloadable<ServiceDiscoveryConfig>,
    ) -> Result<Client> {
        let cur_config = config
            .take()
            .expect("config must be present during client construction");
        let mut client = Client::new_static(service, user_agent, tracer, &cur_config)?;
        client.reload = Some(config);

        Ok(client)
    }

    pub fn new_static(
        service: &str,
        mut user_agent: UserAgent,
        tracer: &Tracer,
        config: &ServiceDiscoveryConfig,
    ) -> Result<Client> {
        user_agent.push_agent(Agent::new("chatter", env!("CARGO_PKG_VERSION")));

        let state = extract_config(service, config)?;

        Ok(Client {
            service: service.to_string(),
            user_agent: HeaderValue::from_str(&user_agent.to_string()).unwrap(),
            tracer: tracer.clone(),
            reload: None,
            state: ArcCell::new(Arc::new(state)),
        })
    }

    fn get_refresh(&self) -> Arc<ClientState> {
        match self.reload.as_ref().and_then(|r| r.take()) {
            Some(config) => match extract_config(&self.service, &config) {
                Ok(state) => {
                    info!("reloaded client for service: {}", self.service);
                    let state = Arc::new(state);
                    self.state.set(state.clone());
                    state
                }
                Err(e) => {
                    error!(
                        "error reloading client, service: {}, error: {}",
                        self.service, e
                    );
                    self.state.get()
                }
            },
            None => self.state.get(),
        }
    }

    /// Creates a new request builder.
    ///
    /// `pattern` is templated - parameters can be filled in via the
    /// `RequestBuilder::param` method.
    pub fn request<'a>(&'a self, method: Method, pattern: &'static str) -> RequestBuilder<'a> {
        RequestBuilder::new(self, pattern, method)
    }

    pub fn get<'a>(&'a self, pattern: &'static str) -> RequestBuilder<'a> {
        self.request(Method::GET, pattern)
    }

    pub fn post<'a>(&'a self, pattern: &'static str) -> RequestBuilder<'a> {
        self.request(Method::POST, pattern)
    }

    pub fn put<'a>(&'a self, pattern: &'static str) -> RequestBuilder<'a> {
        self.request(Method::PUT, pattern)
    }

    pub fn delete<'a>(&'a self, pattern: &'static str) -> RequestBuilder<'a> {
        self.request(Method::DELETE, pattern)
    }

    pub fn patch<'a>(&'a self, pattern: &'static str) -> RequestBuilder<'a> {
        self.request(Method::PATCH, pattern)
    }
}
