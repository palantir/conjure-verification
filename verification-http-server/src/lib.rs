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

#[macro_use]
extern crate futures;
#[macro_use]
extern crate log;

extern crate bytes;
extern crate conjure_verification_error;
extern crate conjure_verification_http;
extern crate core;
extern crate flate2;
extern crate http;
extern crate hyper;
extern crate itertools;
extern crate mime;
extern crate route_recognizer;
extern crate serde_json;
extern crate tokio;
extern crate typed_headers;
extern crate url;

pub use conjure_verification_http::{request, resource, response};

use conjure_verification_error::Result;
use http::status::StatusCode;
use hyper::header::HeaderValue;
use hyper::Method;
use request::Request;
use resource::Resource;
use resource::Route;
use response::IntoResponse;
use response::Response;
use router::Binder;
use std::sync::Arc;

pub mod error_handling;
pub mod handler;
pub mod router;

pub fn register_resource<T>(builder: &mut router::Builder, resource: &Arc<T>)
where
    T: DynamicResource,
{
    let mut binder = Binder::new(resource.clone(), builder, "");
    binder.register_externally(DynamicResource::register);
}

/// Just like `Resource` but allowing the route registration access to `&self`.
pub trait DynamicResource: Resource {
    fn register<R>(&self, router: &mut R)
    where
        R: Route<Self>;
}

/// A trait that I derive automatically for things that have Route<T>, which allows binding a route
/// to the desired method and also to OPTIONS with a default handler for the latter.
pub trait RouteWithOptions<T>: Route<T> {
    /// Creates a route but adds an OPTIONS endpoint to it as well.
    fn route_with_options<F, R>(&mut self, method: Method, route: &str, f: F)
    where
        F: Fn(&T, &mut Request) -> Result<R> + 'static + Sync + Send,
        R: 'static + IntoResponse,
    {
        assert_ne!(method, Method::OPTIONS);
        self.route(method, route, "", f);
        self.route(Method::OPTIONS, route, "", |_, req| Self::options(req));
    }

    /// To support pre-flight requests sent by browsers in CORS mode.
    /// See <https://stackoverflow.com/questions/29954037/why-is-an-options-request-sent-and-can-i-disable-it>
    fn options(_request: &mut Request) -> Result<Response> {
        let mut response: Response = Response::new(StatusCode::OK);
        {
            let headers = &mut response.headers;
            headers.append("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
            headers.append(
                "Access-Control-Allow-Methods",
                HeaderValue::from_static("POST, GET, OPTIONS"),
            );
            headers.append(
                "Access-Control-Allow-Headers",
                // single-header-service.conjure.yml uses 'Some-Header', so we need to whitelist it in preflight checks
                // we also allow 'Fetch-User-Agent' because browsers can't replace User-Agent
                HeaderValue::from_static("Content-Type, Some-Header, Fetch-User-Agent"),
            );
        }
        Ok(response)
    }
}

impl<T, X> RouteWithOptions<T> for X where X: Route<T> {}
