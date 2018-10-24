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
use std::error::Error;
use std::fmt;

use async::alpn::AlpnConnector;

#[derive(Debug)]
pub struct ConnectError(pub Box<Error + Sync + Send>);

impl fmt::Display for ConnectError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, fmt)
    }
}

impl Error for ConnectError {
    fn description(&self) -> &str {
        self.0.description()
    }
}

/// A connector which wraps another and wraps errors in a ConnectError layer.
///
/// This is done so we can determine if an IO error happened during socket
/// connection, in which case we can unconditionally retry.
pub struct CustomErrorConnector(pub AlpnConnector);

impl Connect for CustomErrorConnector {
    type Transport = <AlpnConnector as Connect>::Transport;
    type Error = ConnectError;
    type Future = Box<Future<Item = (Self::Transport, Connected), Error = ConnectError> + Send>;

    fn connect(&self, dst: Destination) -> Self::Future {
        Box::new(self.0.connect(dst).map_err(ConnectError))
    }
}
