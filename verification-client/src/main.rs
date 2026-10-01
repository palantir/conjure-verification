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
#[cfg_attr(test, macro_use)]
extern crate conjure_verification_common;
extern crate conjure_verification_error;
#[macro_use]
extern crate conjure_verification_error_derive;
extern crate conjure_verification_http;
extern crate conjure_verification_http_client;
extern crate conjure_verification_http_server;
extern crate core;
extern crate derive_more;
extern crate either;
extern crate http;
#[macro_use]
extern crate log;
extern crate mime;
extern crate pretty_env_logger;
extern crate serde_conjure;
#[macro_use]
extern crate serde_conjure_derive;
#[macro_use]
extern crate serde_derive;
#[cfg(test)]
extern crate derive_new;
#[cfg_attr(test, macro_use)]
extern crate serde_json;
extern crate serde_plain;
extern crate serde_value;

#[cfg(test)]
#[cfg(test)]
extern crate url;
#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

use crate::resource::VerificationClientResource;
use crate::test_spec::TestCases;
use conjure_verification_common::conjure::ir::Conjure;
use conjure_verification_common::type_mapping;
use conjure_verification_common::type_mapping::return_type;
use conjure_verification_common::type_mapping::ServiceTypeMapping;
use conjure_verification_common::type_mapping::TestType;
use conjure_verification_http::resource::Resource;
use conjure_verification_http_server::router::Binder;
pub use conjure_verification_http_server::*;
use std::env;
use std::env::VarError;
use std::fs::File;
use std::path::Path;
use std::process;
use std::sync::Arc;

mod errors;
mod resource;
mod test_spec;

#[cfg(test)]
mod test;

fn main() {
    pretty_env_logger::init();
    // TODO use clap for arg parsing
    let args = &env::args().collect::<Vec<_>>()[..];
    if args.iter().any(|x| x == "--help") {
        print_usage(&args[0]);
        process::exit(0);
    }

    if args.len() != 3 {
        print_usage(&args[0]);
        process::exit(1);
    }

    let port = match env::var("PORT") {
        Ok(port) => port.parse().unwrap(),
        Err(VarError::NotPresent) => 8000,
        Err(e) => Err(e).unwrap(),
    };

    // Read the test cases file.
    let test_cases_path: &str = &args[1];
    let test_cases = File::open(Path::new(test_cases_path)).unwrap();
    let test_cases: Box<TestCases> = Box::new(serde_json::from_reader(test_cases).unwrap());

    // Read the conjure IR.
    let ir_path: &str = &args[2];
    let ir = File::open(Path::new(ir_path)).unwrap();
    let ir: Box<Conjure> = Box::new(serde_json::from_reader(ir).unwrap());

    let services_mapping = vec![ServiceTypeMapping::new(
        "AutoDeserializeService",
        TestType::Body,
        return_type,
    )];

    let resource = Arc::new(VerificationClientResource::new(
        test_cases.server.into(),
        type_mapping::resolve_types(&ir, &services_mapping).into(),
    ));
    let mut builder = router::Router::builder();
    {
        let ref mut binder = Binder::new(resource.clone(), &mut builder, "");
        VerificationClientResource::register(binder);
    }
    let router = builder.build();

    server::start_server(router, port);
}

fn print_usage(arg0: &str) {
    eprintln!(
        "Usage: {} <client-test-cases.json> <verification-api.conjure.json>",
        arg0
    );
}
