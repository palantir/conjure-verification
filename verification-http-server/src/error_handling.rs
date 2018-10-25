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

use bytes::Bytes;
use conjure_verification_error::Error;
use conjure_verification_http::response::{Body, Response};
use hyper::StatusCode;
use mime::APPLICATION_JSON;
use serde_json;
use typed_headers::{ContentLength, ContentType, HeaderMapExt};

pub fn response(error: &Error) -> Response {
    let status = StatusCode::from_u16(error.code().http_error_code()).unwrap();
    let mut response = Response::new(status);
    let body = serde_json::to_vec(&error).unwrap();
    response
        .headers
        .typed_insert(&ContentType(APPLICATION_JSON));
    response
        .headers
        .typed_insert(&ContentLength(body.len() as u64));
    response.body = Body::Fixed(Bytes::from(body));
    response
}
