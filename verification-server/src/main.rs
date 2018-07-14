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
extern crate bytes;
#[macro_use]
extern crate futures;
extern crate http;
extern crate hyper;
#[macro_use]
extern crate derive_more;
extern crate mime;
extern crate route_recognizer;
extern crate scheduled_thread_pool;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate conjure_verification_error;
extern crate conjure_verification_http;
extern crate core;
extern crate flate2;
extern crate itertools;
extern crate lazy_static;
extern crate pretty_env_logger;
extern crate serde_json;
extern crate serde_value;
extern crate serde_yaml;
extern crate tokio_threadpool;
extern crate typed_headers;
extern crate url;

#[macro_use]
extern crate log;
#[macro_use]
extern crate conjure_verification_error_derive;
extern crate either;
extern crate tokio;

use conjure_verification_http::resource::Resource;
use conjure_verification_http::resource::Route;
use futures::{future, Future};
use handler::HttpService;
use hyper::Server;
use resource::SpecTestResource;
use router::Binder;
use router::Router;
use std::env;
use std::env::VarError;
use std::fs::File;
use std::net::SocketAddr;
use std::path::Path;
use std::process;
use std::sync::Arc;
use test_spec::TestCases;

#[macro_use]
mod macros;
mod error_handling;
mod errors;
mod handler;
mod raw_json;
mod resource;
mod router;
mod test_spec;

fn main() {
    pretty_env_logger::init();

    let port = match env::var("PORT") {
        Ok(port) => port.parse().unwrap(),
        Err(VarError::NotPresent) => 8000,
        e @ Err(_) => e.unwrap(),
    };

    let args = &env::args().collect::<Vec<String>>()[..];
    if args.len() != 2 {
        eprintln!("Usage: {} <test-cases.json>", args[0]);
        process::exit(1);
    }

    if args[1].eq("--help") {
        eprintln!("Usage: {} <test-cases.json>", args[0]);
        process::exit(0);
    }

    // Read the test file.
    let path: &str = &args[1];
    let f = File::open(Path::new(path)).unwrap();
    let test_cases: TestCases = test_spec::from_json_file(f).unwrap();

    let mut builder = router::Router::builder();
    register_resource(
        &mut builder,
        &Arc::new(SpecTestResource::new(test_cases.client)),
    );
    let router = builder.build();

    start_server(router, port);
}

fn start_server(router: Router) {
    // bind to 0.0.0.0 instead of loopback so that requests can be served from docker
    let addr = SocketAddr::new("0.0.0.0".parse().unwrap(), port);

    let router = Arc::new(router);

    hyper::rt::run(future::lazy(move || {
        let new_service = move || future::ok::<_, hyper::Error>(HttpService::new(router.clone()));

        let server = Server::bind(&addr)
            .serve(new_service)
            .map_err(|e| eprintln!("server error: {}", e));

        println!("Listening on http://{}", addr);

        server
    }));
}

fn register_resource<T>(builder: &mut router::Builder, resource: &Arc<T>)
where
    T: DynamicResource,
{
    let mut binder = Binder::new(resource.clone(), builder, "");
    binder.register_externally(DynamicResource::register);
}

/// Just like `Resource` but allowing the route registration access to `&self`.
trait DynamicResource: Resource {
    fn register<R>(&self, router: &mut R)
    where
        R: Route<Self>;
}
