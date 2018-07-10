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
use http::Method;

use request::Request;
use response::IntoResponse;

pub trait Resource: Sized + 'static + Sync + Send {
    const BASE_PATH: &'static str;

    fn register<R>(router: &mut R)
    where
        R: Route<Self>;
}

pub trait Route<T> {
    type NewRoute: NewRoute;

    fn route<F, R>(&mut self, method: Method, route: &str, name: &str, f: F) -> &mut Self::NewRoute
    where
        F: Fn(&T, &mut Request) -> Result<R> + 'static + Sync + Send,
        R: 'static + IntoResponse;

    fn get<F, R>(&mut self, route: &str, name: &str, f: F) -> &mut Self::NewRoute
    where
        F: Fn(&T, &mut Request) -> Result<R> + 'static + Sync + Send,
        R: 'static + IntoResponse,
    {
        self.route(Method::GET, route, name, f)
    }

    fn put<F, R>(&mut self, route: &str, name: &str, f: F) -> &mut Self::NewRoute
    where
        F: Fn(&T, &mut Request) -> Result<R> + 'static + Sync + Send,
        R: 'static + IntoResponse,
    {
        self.route(Method::PUT, route, name, f)
    }

    fn post<F, R>(&mut self, route: &str, name: &str, f: F) -> &mut Self::NewRoute
    where
        F: Fn(&T, &mut Request) -> Result<R> + 'static + Sync + Send,
        R: 'static + IntoResponse,
    {
        self.route(Method::POST, route, name, f)
    }

    fn delete<F, R>(&mut self, route: &str, name: &str, f: F) -> &mut Self::NewRoute
    where
        F: Fn(&T, &mut Request) -> Result<R> + 'static + Sync + Send,
        R: 'static + IntoResponse,
    {
        self.route(Method::DELETE, route, name, f)
    }

    fn patch<F, R>(&mut self, route: &str, name: &str, f: F) -> &mut Self::NewRoute
    where
        F: Fn(&T, &mut Request) -> Result<R> + 'static + Sync + Send,
        R: 'static + IntoResponse,
    {
        self.route(Method::PATCH, route, name, f)
    }
}

pub trait NewRoute {
    fn safe_param(&mut self, param: &str) -> &mut Self;
}
