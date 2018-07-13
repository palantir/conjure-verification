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
extern crate serde_conjure;
#[macro_use]
extern crate serde_conjure_derive;
extern crate conjure_verification_error;
extern crate conjure_verification_http;
extern crate core;
extern crate flate2;
extern crate itertools;
#[macro_use]
extern crate lazy_static;
extern crate chrono;
extern crate pretty_env_logger;
extern crate serde_json;
extern crate serde_plain;
extern crate serde_value;
extern crate serde_yaml;
extern crate tokio_threadpool;
extern crate typed_headers;
extern crate url;
extern crate uuid;

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
use ir::Conjure;
use resource::SpecTestResource;
use router::Binder;
use router::Router;
use std::env;
use std::fs::File;
use std::path::Path;
use std::process;
use std::sync::Arc;
use test_spec::TestCases;

#[macro_use]
mod macros;
mod conjure_serde_value;
mod error_handling;
mod errors;
mod handler;
mod ir;
mod raw_json;
mod resource;
mod router;
mod serde_json_2;
mod test_spec;
mod type_resolution;

fn main() {
    pretty_env_logger::init();
    let args = &env::args().collect::<Vec<_>>()[..];
    if args.iter().any(|x| x == "--help") {
        print_usage(&args[0]);
        process::exit(0);
    }

    if args.len() != 3 {
        print_usage(&args[0]);
        process::exit(1);
    }

    // Read the test cases file.
    let test_cases_path: &str = &args[1];
    let test_cases = File::open(Path::new(test_cases_path)).unwrap();
    let test_cases: Box<TestCases> = Box::new(test_spec::from_json_file(test_cases).unwrap());

    // Read the conjure IR.
    let ir_path: &str = &args[2];
    let ir = File::open(Path::new(ir_path)).unwrap();
    let ir: Box<Conjure> = Box::new(serde_json::from_reader(ir).unwrap());

    let mut builder = router::Router::builder();
    register_resource(
        &mut builder,
        &Arc::new(SpecTestResource::new(test_cases.client.into(), &ir)),
    );
    let router = builder.build();

    start_server(router);
}

fn print_usage(arg0: &String) {
    eprintln!("Usage: {} <test-cases.json> <verification-api.json>", arg0);
}

fn start_server(router: Router) {
    // bind to 0.0.0.0 instead of loopback so that requests can be served from docker
    let addr = "0.0.0.0:8000".parse().unwrap();

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
