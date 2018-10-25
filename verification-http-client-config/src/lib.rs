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

extern crate serde;
extern crate serde_humantime;
extern crate url;
extern crate url_serde;

#[macro_use]
extern crate serde_derive;

#[cfg(test)]
extern crate serde_json;

use serde::de::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use url::Url;

mod raw;
#[cfg(test)]
mod test;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServiceDiscoveryConfig {
    services: HashMap<String, ServiceConfig>,
}

impl<'de> Deserialize<'de> for ServiceDiscoveryConfig {
    fn deserialize<D>(d: D) -> Result<ServiceDiscoveryConfig, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = raw::ServiceDiscoveryConfig::deserialize(d)?;
        Ok(ServiceDiscoveryConfig::from_raw(raw))
    }
}

impl ServiceDiscoveryConfig {
    pub fn builder() -> ServiceDiscoveryConfigBuilder {
        ServiceDiscoveryConfigBuilder::default()
    }

    pub fn service(&self, service: &str) -> Option<&ServiceConfig> {
        self.services.get(service)
    }

    fn from_raw(raw: raw::ServiceDiscoveryConfig) -> ServiceDiscoveryConfig {
        let mut config = ServiceDiscoveryConfig::builder();

        for (name, raw_service) in raw.services {
            let mut service = ServiceConfig::builder();
            service.uris(raw_service.uris);
            if let Some(security) = raw_service.security.as_ref().or(raw.security.as_ref()) {
                service.security(SecurityConfig::from_raw(security));
            }
            if let Some(proxy) = raw_service.proxy.as_ref().or(raw.proxy.as_ref()) {
                service.proxy(ProxyConfig::from_raw(proxy));
            }
            if let Some(connect_timeout) = raw_service.connect_timeout.or(raw.connect_timeout) {
                service.connect_timeout(connect_timeout);
            }
            if let Some(read_timeout) = raw_service.read_timeout.or(raw.read_timeout) {
                service.read_timeout(read_timeout);
            }
            if let Some(write_timeout) = raw_service.write_timeout.or(raw.write_timeout) {
                service.write_timeout(write_timeout);
            }
            if let Some(max_num_retries) = raw_service.max_num_retries {
                service.max_num_retries(max_num_retries);
            }
            if let Some(backoff_slot_size) = raw_service.backoff_slot_size.or(raw.backoff_slot_size)
            {
                service.backoff_slot_size(backoff_slot_size);
            }
            if let Some(keep_alive) = raw_service.keep_alive.or(raw.keep_alive) {
                service.keep_alive(keep_alive);
            }
            if let Some(experimental_assume_http2) = raw_service
                .experimental_assume_http2
                .or(raw.experimental_assume_http2)
            {
                service.experimental_assume_http2(experimental_assume_http2);
            }

            config.service(&name, service.build());
        }

        config.build()
    }
}

pub struct ServiceDiscoveryConfigBuilder(ServiceDiscoveryConfig);

impl Default for ServiceDiscoveryConfigBuilder {
    fn default() -> ServiceDiscoveryConfigBuilder {
        ServiceDiscoveryConfigBuilder(ServiceDiscoveryConfig::default())
    }
}

impl From<ServiceDiscoveryConfig> for ServiceDiscoveryConfigBuilder {
    fn from(config: ServiceDiscoveryConfig) -> ServiceDiscoveryConfigBuilder {
        ServiceDiscoveryConfigBuilder(config)
    }
}

impl ServiceDiscoveryConfigBuilder {
    pub fn service(&mut self, name: &str, config: ServiceConfig) -> &mut Self {
        self.0.services.insert(name.to_string(), config);
        self
    }

    pub fn build(&self) -> ServiceDiscoveryConfig {
        self.0.clone()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceConfig {
    uris: Vec<Url>,
    security: SecurityConfig,
    connect_timeout: Duration,
    read_timeout: Duration,
    write_timeout: Duration,
    max_num_retries: u32,
    backoff_slot_size: Duration,
    proxy: ProxyConfig,
    keep_alive: bool,
    experimental_assume_http2: bool,
}

impl Default for ServiceConfig {
    fn default() -> ServiceConfig {
        ServiceConfig {
            uris: vec![],
            security: SecurityConfig::builder().build(),
            proxy: ProxyConfig::Direct,
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(10 * 60),
            write_timeout: Duration::from_secs(10 * 60),
            backoff_slot_size: Duration::from_millis(250),
            max_num_retries: 3,
            keep_alive: true,
            experimental_assume_http2: false,
        }
    }
}

impl ServiceConfig {
    pub fn builder() -> ServiceConfigBuilder {
        ServiceConfigBuilder::default()
    }

    pub fn uris(&self) -> &[Url] {
        &self.uris
    }

    pub fn security(&self) -> &SecurityConfig {
        &self.security
    }

    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    pub fn read_timeout(&self) -> Duration {
        self.read_timeout
    }

    pub fn write_timeout(&self) -> Duration {
        self.write_timeout
    }

    pub fn max_num_retries(&self) -> u32 {
        self.max_num_retries
    }

    pub fn backoff_slot_size(&self) -> Duration {
        self.backoff_slot_size
    }

    pub fn proxy(&self) -> &ProxyConfig {
        &self.proxy
    }

    pub fn keep_alive(&self) -> bool {
        self.keep_alive
    }

    pub fn experimental_assume_http2(&self) -> bool {
        self.experimental_assume_http2
    }
}

pub struct ServiceConfigBuilder(ServiceConfig);

impl Default for ServiceConfigBuilder {
    fn default() -> ServiceConfigBuilder {
        ServiceConfigBuilder(ServiceConfig::default())
    }
}

impl From<ServiceConfig> for ServiceConfigBuilder {
    fn from(config: ServiceConfig) -> ServiceConfigBuilder {
        ServiceConfigBuilder(config)
    }
}

impl ServiceConfigBuilder {
    pub fn uris(&mut self, uris: Vec<Url>) -> &mut Self {
        self.0.uris = uris;
        self
    }

    pub fn security(&mut self, security: SecurityConfig) -> &mut Self {
        self.0.security = security;
        self
    }

    pub fn connect_timeout(&mut self, connect_timeout: Duration) -> &mut Self {
        self.0.connect_timeout = connect_timeout;
        self
    }

    pub fn read_timeout(&mut self, read_timeout: Duration) -> &mut Self {
        self.0.read_timeout = read_timeout;
        self
    }

    pub fn write_timeout(&mut self, write_timeout: Duration) -> &mut Self {
        self.0.write_timeout = write_timeout;
        self
    }

    pub fn max_num_retries(&mut self, max_num_retries: u32) -> &mut Self {
        self.0.max_num_retries = max_num_retries;
        self
    }

    pub fn backoff_slot_size(&mut self, backoff_slot_size: Duration) -> &mut Self {
        self.0.backoff_slot_size = backoff_slot_size;
        self
    }

    pub fn proxy(&mut self, proxy: ProxyConfig) -> &mut Self {
        self.0.proxy = proxy;
        self
    }

    pub fn keep_alive(&mut self, keep_alive: bool) -> &mut Self {
        self.0.keep_alive = keep_alive;
        self
    }

    pub fn experimental_assume_http2(&mut self, experimental_assume_http2: bool) -> &mut Self {
        self.0.experimental_assume_http2 = experimental_assume_http2;
        self
    }

    pub fn build(&self) -> ServiceConfig {
        self.0.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SecurityConfig {
    ca_file: Option<PathBuf>,
}

impl SecurityConfig {
    pub fn builder() -> SecurityConfigBuilder {
        SecurityConfigBuilder::default()
    }

    pub fn ca_file(&self) -> Option<&Path> {
        self.ca_file.as_ref().map(|p| &**p)
    }

    fn from_raw(raw: &raw::SecurityConfig) -> SecurityConfig {
        let mut config = SecurityConfig::builder();
        if let Some(ref ca_file) = raw.ca_file {
            config.ca_file(Some(ca_file.to_path_buf()));
        }
        config.build()
    }
}

pub struct SecurityConfigBuilder(SecurityConfig);

impl Default for SecurityConfigBuilder {
    fn default() -> SecurityConfigBuilder {
        SecurityConfigBuilder(SecurityConfig::default())
    }
}

impl From<SecurityConfig> for SecurityConfigBuilder {
    fn from(config: SecurityConfig) -> SecurityConfigBuilder {
        SecurityConfigBuilder(config)
    }
}

impl SecurityConfigBuilder {
    pub fn ca_file(&mut self, ca_file: Option<PathBuf>) -> &mut Self {
        self.0.ca_file = ca_file;
        self
    }

    pub fn build(&self) -> SecurityConfig {
        self.0.clone()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProxyConfig {
    Direct,
    Http(HttpProxyConfig),
    Mesh(MeshProxyConfig),
    #[doc(hidden)]
    __ForExtensibility,
}

impl Default for ProxyConfig {
    fn default() -> ProxyConfig {
        ProxyConfig::Direct
    }
}

impl ProxyConfig {
    fn from_raw(raw: &raw::ProxyConfig) -> ProxyConfig {
        match *raw {
            raw::ProxyConfig::Http {
                ref host_and_port,
                ref credentials,
            } => {
                let mut builder = HttpProxyConfig::builder();
                builder.host_and_port(HostAndPort::from_raw(host_and_port));
                if let Some(ref credentials) = *credentials {
                    builder.credentials(Some(BasicCredentials::from_raw(credentials)));
                }
                ProxyConfig::Http(builder.build())
            }
            raw::ProxyConfig::Mesh { ref host_and_port } => {
                let config = MeshProxyConfig::builder()
                    .host_and_port(HostAndPort::from_raw(host_and_port))
                    .build();
                ProxyConfig::Mesh(config)
            }
            raw::ProxyConfig::Direct {} => ProxyConfig::Direct,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpProxyConfig {
    host_and_port: HostAndPort,
    credentials: Option<BasicCredentials>,
}

impl HttpProxyConfig {
    pub fn builder() -> HttpProxyConfigBuilder {
        HttpProxyConfigBuilder::default()
    }

    pub fn host_and_port(&self) -> &HostAndPort {
        &self.host_and_port
    }

    pub fn credentials(&self) -> Option<&BasicCredentials> {
        self.credentials.as_ref()
    }
}

pub struct HttpProxyConfigBuilder {
    host_and_port: Option<HostAndPort>,
    credentials: Option<BasicCredentials>,
}

impl Default for HttpProxyConfigBuilder {
    fn default() -> HttpProxyConfigBuilder {
        HttpProxyConfigBuilder {
            host_and_port: None,
            credentials: None,
        }
    }
}

impl From<HttpProxyConfig> for HttpProxyConfigBuilder {
    fn from(config: HttpProxyConfig) -> HttpProxyConfigBuilder {
        HttpProxyConfigBuilder {
            host_and_port: Some(config.host_and_port),
            credentials: config.credentials,
        }
    }
}

impl HttpProxyConfigBuilder {
    pub fn host_and_port(&mut self, host_and_port: HostAndPort) -> &mut Self {
        self.host_and_port = Some(host_and_port);
        self
    }

    pub fn credentials(&mut self, credentials: Option<BasicCredentials>) -> &mut Self {
        self.credentials = credentials;
        self
    }

    pub fn build(&self) -> HttpProxyConfig {
        HttpProxyConfig {
            host_and_port: self.host_and_port.clone().expect("host_and_port not set"),
            credentials: self.credentials.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HostAndPort {
    host: String,
    port: u16,
}

impl fmt::Display for HostAndPort {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}:{}", self.host, self.port)
    }
}

impl HostAndPort {
    pub fn new(host: &str, port: u16) -> HostAndPort {
        HostAndPort {
            host: host.to_string(),
            port,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    fn from_raw(raw: &raw::HostAndPort) -> HostAndPort {
        HostAndPort::new(&raw.host, raw.port)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BasicCredentials {
    username: String,
    password: String,
}

impl BasicCredentials {
    pub fn new(username: &str, password: &str) -> BasicCredentials {
        BasicCredentials {
            username: username.to_string(),
            password: password.to_string(),
        }
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    fn from_raw(raw: &raw::BasicCredentials) -> BasicCredentials {
        BasicCredentials::new(&raw.username, &raw.password)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeshProxyConfig {
    host_and_port: HostAndPort,
}

impl MeshProxyConfig {
    pub fn builder() -> MeshProxyConfigBuilder {
        MeshProxyConfigBuilder::default()
    }

    pub fn host_and_port(&self) -> &HostAndPort {
        &self.host_and_port
    }
}

pub struct MeshProxyConfigBuilder {
    host_and_port: Option<HostAndPort>,
}

impl Default for MeshProxyConfigBuilder {
    fn default() -> MeshProxyConfigBuilder {
        MeshProxyConfigBuilder {
            host_and_port: None,
        }
    }
}

impl From<MeshProxyConfig> for MeshProxyConfigBuilder {
    fn from(config: MeshProxyConfig) -> MeshProxyConfigBuilder {
        MeshProxyConfigBuilder {
            host_and_port: Some(config.host_and_port),
        }
    }
}

impl MeshProxyConfigBuilder {
    pub fn host_and_port(&mut self, host_and_port: HostAndPort) -> &mut Self {
        self.host_and_port = Some(host_and_port);
        self
    }

    pub fn build(&self) -> MeshProxyConfig {
        MeshProxyConfig {
            host_and_port: self.host_and_port.clone().expect("host_and_port not set"),
        }
    }
}
