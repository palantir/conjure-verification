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

use hyper::Uri;
use hyper_openssl::client::legacy::{HttpsConnector, MaybeHttpsStream};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower_service::Service;

use crate::r#async::proxy::{ConnStream, ProxyConnector};

#[derive(Clone)]
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

type BoxError = Box<dyn Error + Sync + Send>;

impl Service<Uri> for AlpnConnector {
    type Response = MaybeHttpsStream<ConnStream>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, dst: Uri) -> Self::Future {
        let mut connector = self.connector.clone();
        let require_http2 = self.require_http2;
        Box::pin(async move {
            let stream = connector.call(dst).await?;
            if let MaybeHttpsStream::Https(ref stream) = stream {
                if require_http2 && stream.ssl().selected_alpn_protocol() != Some(b"h2") {
                    return Err("failed to select h2 in ALPN".into());
                }
            }
            Ok(stream)
        })
    }
}
