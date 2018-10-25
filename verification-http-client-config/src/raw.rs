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

use serde::de::{Deserialize, Deserializer, Error, SeqAccess, Unexpected, Visitor};
use serde_humantime;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;
use url::Url;
use url_serde;

#[derive(Deserialize, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct ServiceDiscoveryConfig {
    pub services: HashMap<String, ServiceConfig>,
    pub security: Option<SecurityConfig>,
    pub proxy: Option<ProxyConfig>,
    #[serde(deserialize_with = "de_opt_duration")]
    pub connect_timeout: Option<Duration>,
    #[serde(deserialize_with = "de_opt_duration")]
    pub read_timeout: Option<Duration>,
    #[serde(deserialize_with = "de_opt_duration")]
    pub write_timeout: Option<Duration>,
    #[serde(deserialize_with = "de_opt_duration")]
    pub backoff_slot_size: Option<Duration>,
    pub keep_alive: Option<bool>,
    pub experimental_assume_http2: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SecurityConfig {
    pub ca_file: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum ProxyConfig {
    #[serde(rename_all = "kebab-case")]
    Http {
        host_and_port: HostAndPort,
        credentials: Option<BasicCredentials>,
    },
    #[serde(rename_all = "kebab-case")]
    Mesh {
        host_and_port: HostAndPort,
    },
    Direct {},
}

pub struct HostAndPort {
    pub host: String,
    pub port: u16,
}

impl<'de> Deserialize<'de> for HostAndPort {
    fn deserialize<D>(deserializer: D) -> Result<HostAndPort, D::Error>
    where
        D: Deserializer<'de>,
    {
        let expected = "a host:port identifier";

        let mut s = String::deserialize(deserializer)?;
        match s.find(":") {
            Some(idx) => {
                let port = s[idx + 1..]
                    .parse()
                    .map_err(|_| D::Error::invalid_value(Unexpected::Str(&s), &expected))?;
                s.truncate(idx);
                Ok(HostAndPort { host: s, port })
            }
            None => Err(D::Error::invalid_value(Unexpected::Str(&s), &expected)),
        }
    }
}

#[derive(Deserialize)]
pub struct BasicCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ServiceConfig {
    #[serde(default)]
    pub security: Option<SecurityConfig>,
    #[serde(deserialize_with = "de_urls")]
    pub uris: Vec<Url>,
    #[serde(deserialize_with = "de_opt_duration", default)]
    pub connect_timeout: Option<Duration>,
    #[serde(deserialize_with = "de_opt_duration", default)]
    pub read_timeout: Option<Duration>,
    #[serde(deserialize_with = "de_opt_duration", default)]
    pub write_timeout: Option<Duration>,
    pub max_num_retries: Option<u32>,
    #[serde(deserialize_with = "de_opt_duration", default)]
    pub backoff_slot_size: Option<Duration>,
    #[serde(default)]
    pub proxy: Option<ProxyConfig>,
    #[serde(default)]
    pub keep_alive: Option<bool>,
    #[serde(default)]
    pub experimental_assume_http2: Option<bool>,
}

fn de_urls<'de, D>(d: D) -> Result<Vec<Url>, D::Error>
where
    D: Deserializer<'de>,
{
    struct V;

    impl<'de2> Visitor<'de2> for V {
        type Value = Vec<Url>;

        fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
            fmt.write_str("a list")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Vec<Url>, A::Error>
        where
            A: SeqAccess<'de2>,
        {
            let mut vs = vec![];
            while let Some(url) = seq.next_element::<url_serde::De<Url>>()? {
                vs.push(url.into_inner());
            }
            Ok(vs)
        }
    }

    d.deserialize_seq(V)
}

fn de_opt_duration<'de, D>(d: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    serde_humantime::De::deserialize(d).map(|d| d.into_inner())
}
