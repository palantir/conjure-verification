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

use crate::handler::HttpService;
use crate::router::Router;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

/// Binds to `port` on all interfaces and serves `router` forever.
pub fn start_server(router: Router, port: u16) {
    // bind to 0.0.0.0 instead of loopback so that requests can be served from docker
    let addr = SocketAddr::new("0.0.0.0".parse().unwrap(), port);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async move {
        let listener = TcpListener::bind(addr).await.unwrap();
        println!("Listening on http://{}", addr);
        serve(listener, Arc::new(router)).await
    });
}

/// Serves HTTP/1 and HTTP/2 (prior knowledge) connections accepted from `listener` forever.
pub async fn serve(listener: TcpListener, router: Arc<Router>) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(s) => s,
            // the connection was closed before we accepted it
            Err(ref e) if is_connection_error(e) => continue,
            Err(e) => {
                // e.g. too many open files: back off instead of spinning, as hyper 0.12's server did
                eprintln!("accept error: {}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        let io = TokioIo::new(stream);
        let service = HttpService::new(router.clone());
        let svc = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
            let service = service.clone();
            async move { service.call(req).await }
        });
        tokio::spawn(async move {
            let builder = auto::Builder::new(TokioExecutor::new());
            if let Err(e) = builder.serve_connection(io, svc).await {
                eprintln!("connection error: {}", e);
            }
        });
    }
}

fn is_connection_error(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
    )
}
