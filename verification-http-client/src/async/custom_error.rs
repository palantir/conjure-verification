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
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower_service::Service;

use crate::r#async::alpn::AlpnConnector;

#[derive(Debug)]
pub struct ConnectError(pub Box<dyn Error + Sync + Send>);

impl fmt::Display for ConnectError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, fmt)
    }
}

impl Error for ConnectError {}

/// A connector which wraps another and wraps errors in a ConnectError layer.
///
/// This is done so we can determine if an IO error happened during socket
/// connection, in which case we can unconditionally retry.
#[derive(Clone)]
pub struct CustomErrorConnector(pub AlpnConnector);

impl Service<Uri> for CustomErrorConnector {
    type Response = <AlpnConnector as Service<Uri>>::Response;
    type Error = ConnectError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, dst: Uri) -> Self::Future {
        let mut inner = self.0.clone();
        Box::pin(async move { inner.call(dst).await.map_err(ConnectError) })
    }
}
