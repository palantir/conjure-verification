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
use conjure_verification_error::Result;
use conjure_verification_http::request::Request;
use conjure_verification_http::resource::{NewRoute, Resource, Route};
use conjure_verification_http::response::{IntoResponse, Response};
use hyper::Method;
use route_recognizer::{self, Params};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;

pub trait Handle {
    fn handle(&self, request: &mut Request) -> Result<Response>;
}

pub struct Endpoint {
    pub handler: Box<Handle + Sync + Send>,
}

impl NewRoute for Endpoint {
    fn safe_param(&mut self, _param: &str) -> &mut Endpoint {
        unimplemented!()
    }
}

struct Pattern {
    pattern: Arc<str>,
    endpoints: HashMap<Method, Arc<Endpoint>>,
}

pub struct Router {
    router: route_recognizer::Router<Pattern>,
}

impl Router {
    pub fn builder() -> Builder {
        Builder(HashMap::new())
    }

    pub fn route(&self, method: &Method, path: &str) -> RouteResult {
        let path = path.trim_right_matches('/');

        let matches = match self.router.recognize(path) {
            Ok(matches) => matches,
            Err(_) => return RouteResult::NotFound,
        };

        match matches.handler.endpoints.get(&method) {
            Some(endpoint) => RouteResult::Matched {
                pattern: matches.handler.pattern.clone(),
                params: matches.params,
                endpoint: endpoint.clone(),
            },
            None => {
                RouteResult::MethodNotAllowed(matches.handler.endpoints.keys().cloned().collect())
            }
        }
    }
}

pub enum RouteResult {
    Matched {
        pattern: Arc<str>,
        params: Params,
        endpoint: Arc<Endpoint>,
    },
    MethodNotAllowed(Vec<Method>),
    NotFound,
}

pub struct Builder(HashMap<Arc<str>, HashMap<Method, Endpoint>>);

impl Builder {
    pub fn route(&mut self, method: Method, pattern: String, endpoint: Endpoint) -> &mut Endpoint {
        match self
            .0
            .entry(pattern.into())
            .or_insert_with(HashMap::new)
            .entry(method)
        {
            Entry::Occupied(e) => {
                let e = e.into_mut();
                *e = endpoint;
                e
            }
            Entry::Vacant(e) => e.insert(endpoint),
        }
    }

    pub fn build(self) -> Router {
        let mut router = route_recognizer::Router::new();
        for (pattern, endpoints) in self.0 {
            let value = Pattern {
                pattern: pattern.clone(),
                endpoints: endpoints
                    .into_iter()
                    .map(|(k, v)| (k, Arc::new(v)))
                    .collect(),
            };
            router.add(&pattern, value);
        }
        Router { router }
    }
}

pub struct Binder<'a, T> {
    resource: Arc<T>,
    router: &'a mut Builder,
    prefix: &'static str,
}

impl<'a, T> Binder<'a, T>
where
    T: Resource,
{
    pub fn new(resource: Arc<T>, router: &'a mut Builder, prefix: &'static str) -> Binder<'a, T> {
        validate_path(T::BASE_PATH);
        Binder {
            resource,
            router,
            prefix,
        }
    }

    pub fn register_externally<F>(&mut self, f: F)
    where
        F: FnOnce(&T, &mut Self),
    {
        f(&self.resource.clone(), self)
    }

    fn route_inner(
        &mut self,
        method: Method,
        route: &str,
        name: &str,
        handler: Box<Handle + Sync + Send>,
    ) -> &mut Endpoint {
        add_route(
            self.router,
            self.prefix,
            T::BASE_PATH,
            method,
            route,
            name,
            handler,
        )
    }
}

impl<'a, T> Route<T> for Binder<'a, T>
where
    T: Resource,
{
    type NewRoute = Endpoint;

    fn route<F, R>(&mut self, method: Method, route: &str, name: &str, f: F) -> &mut Endpoint
    where
        F: Fn(&T, &mut Request) -> Result<R> + 'static + Sync + Send,
        R: 'static + IntoResponse,
    {
        let handler = Box::new(Handler {
            resource: self.resource.clone(),
            f,
        });
        self.route_inner(method, route, name, handler)
    }
}

fn add_route<'a>(
    router: &'a mut Builder,
    prefix: &str,
    base_path: &str,
    method: Method,
    route: &str,
    _endpoint: &str,
    handler: Box<Handle + Sync + Send>,
) -> &'a mut Endpoint {
    validate_path(route);

    let endpoint = Endpoint { handler };

    let route = format!("{}{}{}", prefix, base_path, route);
    router.route(method, route, endpoint)
}

fn validate_path(path: &str) {
    assert!(
        path.is_empty() || (path.starts_with('/') && !path.ends_with('/')),
        "path must either be empty or start but not end with `/`: {}",
        path
    );
}

struct Handler<T, F> {
    resource: Arc<T>,
    f: F,
}

impl<T, F, R> Handle for Handler<T, F>
where
    T: 'static + Sync + Send,
    F: Fn(&T, &mut Request) -> Result<R> + 'static + Sync + Send,
    R: 'static + IntoResponse,
{
    fn handle(&self, request: &mut Request) -> Result<Response> {
        (self.f)(&self.resource, request).and_then(|r| r.into_response(request))
    }
}
