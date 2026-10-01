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

use std::error::Error;
use std::io;
use std::net::ToSocketAddrs;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Copy, Clone)]
pub struct Timeouts {
    pub connect: Duration,
    pub read: Duration,
    pub write: Duration,
}

#[derive(Copy, Clone)]
pub struct SocketConnector(pub Timeouts);

impl SocketConnector {
    pub async fn connect(
        &self,
        host: &str,
        port: u16,
    ) -> Result<tokio::net::TcpStream, Box<dyn Error + Sync + Send>> {
        let host = host.to_string();
        let timeouts = self.0;

        let addrs = tokio::task::spawn_blocking(move || {
            debug!("resolving addresses, host: {}, port: {}", host, port);
            (&*host, port).to_socket_addrs()
        })
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Sync + Send>)??;

        let mut last_err: Option<Box<dyn Error + Sync + Send>> = None;
        for addr in addrs {
            debug!("connecting to server, addr: {}", addr);
            match timeout(timeouts.connect, TcpStream::connect(addr)).await {
                Ok(Ok(stream)) => {
                    stream.set_nodelay(true)?;
                    let keepalive = Duration::min(timeouts.read, timeouts.write);
                    let sock_ref = socket2::SockRef::from(&stream);
                    let mut ka = socket2::TcpKeepalive::new();
                    ka = ka.with_time(keepalive);
                    let _ = sock_ref.set_tcp_keepalive(&ka);
                    debug!("connected to server, addr: {}", addr);
                    return Ok(stream);
                }
                Ok(Err(e)) => {
                    debug!("error connecting to server, addr: {}, error: {}", addr, e);
                    last_err = Some(Box::new(e));
                }
                Err(_) => {
                    debug!("connection timed out, addr: {}", addr);
                    last_err = Some(Box::new(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "connection timed out",
                    )));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            Box::new(io::Error::new(
                io::ErrorKind::Other,
                "resolved 0 addresses from hostname",
            ))
        }))
    }
}
