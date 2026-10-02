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

#[macro_use]
extern crate log;

use crate::config::{HostAndPort, ProxyConfig, ServiceDiscoveryConfig};
use crate::errors::{Error, Result, SerializableError};
use arc_swap::ArcSwap;
use hyper::header::HeaderValue;
use hyper::{Method, StatusCode};
use hyper_openssl::client::legacy::HttpsConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use mime::Mime;
use openssl::error::ErrorStack;
use openssl::ssl::{SslConnector, SslConnectorBuilder, SslMethod};
use std::error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::runtime::Runtime;

pub use crate::body::*;
use crate::node_selector::NodeSelector;
use crate::r#async::alpn::AlpnConnector;
use crate::r#async::custom_error::CustomErrorConnector;
use crate::r#async::proxy::{ProxyConnector, ProxyConnectorConfig};
use crate::r#async::socket::{SocketConnector, Timeouts};
pub use crate::reloadable::*;
pub use crate::request::*;
pub use crate::response::*;
pub use crate::user_agent::*;

#[doc(inline)]
pub use hyper::header;

pub mod config {
    pub use conjure_verification_http_client_config::*;
}

mod errors {
    pub use conjure_verification_error::*;
}

pub mod r#async;
pub mod backoff;
pub mod body;
pub mod node_selector;
pub mod reloadable;
pub mod request;
pub mod response;
pub mod user_agent;

#[cfg(test)]
mod test;

static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| Runtime::new().unwrap());
static APPLICATION_CBOR: LazyLock<Mime> = LazyLock::new(|| "application/cbor".parse().unwrap());

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

    let mut ssl = ssl_connector()?;

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
            let credentials = config
                .credentials()
                .map(|c| (c.username().to_string(), c.password().to_string()));

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

    let mut builder = hyper_util::client::legacy::Builder::new(TokioExecutor::new());
    builder
        .pool_timer(TokioTimer::new())
        .pool_idle_timeout(Duration::from_secs(90))
        .http2_only(service_config.experimental_assume_http2());
    if !service_config.keep_alive() {
        builder.pool_max_idle_per_host(0);
    }
    let client = builder.build(connector);

    Ok(ClientState {
        client,
        nodes,
        max_num_retries: service_config.max_num_retries(),
        backoff_slot_size: service_config.backoff_slot_size(),
        proxy: proxy_state,
    })
}

fn ssl_connector() -> Result<SslConnectorBuilder> {
    let mut ssl = SslConnector::builder(SslMethod::tls()).map_err(Error::internal_safe)?;

    // OpenSSL is statically linked, so its default certificate locations aren't the OS's.
    let probe = openssl_probe::probe();
    if let Some(ref cert_file) = probe.cert_file.or_else(macos_cert_file) {
        ssl.load_verify_locations(Some(cert_file), None)
            .map_err(Error::internal_safe)?;
    }
    for cert_dir in &probe.cert_dir {
        ssl.load_verify_locations(None, Some(cert_dir))
            .map_err(Error::internal_safe)?;
    }
    // https://github.com/openssl/openssl/issues/6851
    ErrorStack::get();

    Ok(ssl)
}

/// The certificate bundle shipped with macOS, which `openssl_probe` doesn't look for.
fn macos_cert_file() -> Option<PathBuf> {
    let path = Path::new("/etc/ssl/cert.pem");
    if cfg!(target_os = "macos") && path.exists() {
        Some(path.to_path_buf())
    } else {
        None
    }
}

struct ClientState {
    client: hyper_util::client::legacy::Client<
        CustomErrorConnector,
        http_body_util::Full<bytes::Bytes>,
    >,
    nodes: NodeSelector,
    max_num_retries: u32,
    backoff_slot_size: Duration,
    proxy: Option<ProxyState>,
}

pub(crate) type ProxyAuthorization = (String, String);

enum ProxyState {
    Http {
        credentials: Option<(String, String)>,
    },
    Mesh {
        host: HostAndPort,
    },
}

/// An HTTP client to a remote service.
pub struct Client {
    service: String,
    user_agent: HeaderValue,
    reload: Option<Reloadable<ServiceDiscoveryConfig>>,
    state: ArcSwap<ClientState>,
}

impl Client {
    pub fn new(
        service: &str,
        user_agent: UserAgent,
        config: Reloadable<ServiceDiscoveryConfig>,
    ) -> Result<Client> {
        let cur_config = config
            .take()
            .expect("config must be present during client construction");
        let mut client = Client::new_static(service, user_agent, &cur_config)?;
        client.reload = Some(config);

        Ok(client)
    }

    pub fn new_static(
        service: &str,
        mut user_agent: UserAgent,
        config: &ServiceDiscoveryConfig,
    ) -> Result<Client> {
        user_agent.push_agent(Agent::new("chatter", env!("CARGO_PKG_VERSION")));

        let state = extract_config(service, config)?;

        Ok(Client {
            service: service.to_string(),
            user_agent: HeaderValue::from_str(&user_agent.to_string()).unwrap(),
            reload: None,
            state: ArcSwap::new(Arc::new(state)),
        })
    }

    fn get_refresh(&self) -> Arc<ClientState> {
        match self.reload.as_ref().and_then(|r| r.take()) {
            Some(config) => match extract_config(&self.service, &config) {
                Ok(state) => {
                    info!("reloaded client for service: {}", self.service);
                    let state = Arc::new(state);
                    self.state.store(state.clone());
                    state
                }
                Err(e) => {
                    error!(
                        "error reloading client, service: {}, error: {}",
                        self.service, e
                    );
                    self.state.load_full()
                }
            },
            None => self.state.load_full(),
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
