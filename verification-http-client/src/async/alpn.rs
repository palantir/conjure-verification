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

use futures::Future;
use hyper::client::connect::{Connect, Connected, Destination};
use hyper_openssl::{HttpsConnector, MaybeHttpsStream};
use std::error::Error;

use async::proxy::ProxyConnector;

pub struct AlpnConnector {
    connector: HttpsConnector<ProxyConnector>,
    require_http2: bool,
}

impl AlpnConnector {
    pub fn new(connector: HttpsConnector<ProxyConnector>, require_http2: bool) -> AlpnConnector {
        AlpnConnector {
            connector,
            require_http2,
        }
    }
}

impl Connect for AlpnConnector {
    type Transport = <HttpsConnector<ProxyConnector> as Connect>::Transport;
    type Error = Box<Error + Sync + Send>;
    type Future =
        Box<Future<Item = (Self::Transport, Connected), Error = Box<Error + Sync + Send>> + Send>;

    fn connect(&self, dst: Destination) -> Self::Future {
        let require_http2 = self.require_http2;
        let f = self
            .connector
            .connect(dst)
            .and_then(move |(stream, connected)| {
                if let MaybeHttpsStream::Https(ref stream) = stream {
                    if require_http2
                        && stream.get_ref().ssl().selected_alpn_protocol() != Some(b"h2")
                    {
                        return Err("failed to select h2 in ALPN".into());
                    }
                }

                Ok((stream, connected))
            });

        Box::new(f)
    }
}
