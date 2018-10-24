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

use futures::{Async, Future, Poll};
use state_machine_future::RentToOwn;
use std::error::Error;
use std::io;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::time::Duration;
use std::vec;
use tokio::net::ConnectFuture;
use tokio::net::TcpStream;
use tokio::timer::Timeout;
use tokio_io_timeout::TimeoutStream;
use tokio_threadpool;

#[derive(Copy, Clone)]
pub struct Timeouts {
    pub connect: Duration,
    pub read: Duration,
    pub write: Duration,
}

#[derive(Copy, Clone)]
pub struct SocketConnector(pub Timeouts);

impl SocketConnector {
    pub fn connect(&self, host: &str, port: u16) -> SocketConnectFuture {
        SocketConnect::start(host.to_string(), port, self.0)
    }
}

#[derive(StateMachineFuture)]
pub enum SocketConnect {
    #[state_machine_future(start, transitions(Connecting))]
    Start {
        host: String,
        port: u16,
        timeouts: Timeouts,
    },
    #[state_machine_future(transitions(Ready))]
    Connecting {
        addrs: vec::IntoIter<SocketAddr>,
        cur: Timeout<ConnectFuture>,
        cur_addr: SocketAddr,
        timeouts: Timeouts,
    },
    #[state_machine_future(ready)]
    Ready(TimeoutStream<TcpStream>),
    #[state_machine_future(error)]
    Failed(Box<Error + Sync + Send>),
}

impl PollSocketConnect for SocketConnect {
    fn poll_start<'a>(
        start: &'a mut RentToOwn<'a, Start>,
    ) -> Poll<AfterStart, Box<Error + Sync + Send>> {
        let mut addrs = try_ready!(tokio_threadpool::blocking(|| {
            debug!(
                "resolving addresses, host: {}, port: {}",
                start.host, start.port
            );
            (&*start.host, start.port).to_socket_addrs()
        }))?;

        let addr = match addrs.next() {
            Some(addr) => addr,
            None => {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::Other,
                    "resolved 0 addresses from hostname",
                )))
            }
        };
        debug!("connecting to server, addr: {}", addr);

        let connect = TcpStream::connect(&addr);
        let cur = Timeout::new(connect, start.timeouts.connect);

        transition!(Connecting {
            addrs,
            cur,
            cur_addr: addr,
            timeouts: start.timeouts,
        })
    }

    fn poll_connecting<'a>(
        connecting: &'a mut RentToOwn<'a, Connecting>,
    ) -> Poll<AfterConnecting, Box<Error + Sync + Send>> {
        loop {
            let r = match connecting.cur.poll() {
                Ok(Async::Ready(stream)) => Ok(stream),
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(e) => match e.into_inner() {
                    Some(e) => Err(e),
                    None => Err(io::Error::new(io::ErrorKind::Other, "connection timed out")),
                },
            };

            match r {
                Ok(stream) => {
                    stream.set_nodelay(true)?;
                    let timeout =
                        Duration::min(connecting.timeouts.read, connecting.timeouts.write);
                    stream.set_keepalive(Some(timeout))?;

                    let mut stream = TimeoutStream::new(stream);
                    stream.set_read_timeout(Some(connecting.timeouts.read));
                    stream.set_write_timeout(Some(connecting.timeouts.write));
                    debug!("connected to server, addr: {}", connecting.cur_addr);
                    transition!(Ready(stream));
                }
                Err(e) => {
                    debug!(
                        "error connecting to server, addr: {}, error: {}",
                        connecting.cur_addr, e
                    );
                    match connecting.addrs.next() {
                        Some(addr) => {
                            debug!("connecting to server, addr: {}", addr);
                            connecting.cur_addr = addr;
                            let connect = TcpStream::connect(&addr);
                            connecting.cur = Timeout::new(connect, connecting.timeouts.connect);
                        }
                        None => return Err(Box::new(e)),
                    }
                }
            }
        }
    }
}
