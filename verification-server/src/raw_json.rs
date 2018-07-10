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
use conjure_verification_error::Result;
use conjure_verification_http::request::{Format, Request};
use conjure_verification_http::response::IntoResponse;
use conjure_verification_http::response::{Body, Response};
use conjure_verification_http::SerializableFormat;
use http::header::HeaderValue;
use http::StatusCode;
use typed_headers::{ContentLength, ContentType, HeaderMapExt};

pub struct RawJson {
    pub data: Bytes,
}

impl IntoResponse for RawJson {
    fn into_response(self, request: &Request) -> Result<Response> {
        // Ensure that the client accepts JSON.
        let format = *request.response_format(&[SerializableFormat::Json])?;

        let mut response = Response::new(StatusCode::OK);
        response
            .headers
            .typed_insert(&ContentType(format.mime().clone()));
        response
            .headers
            .append("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
        response
            .headers
            .typed_insert(&ContentLength(self.data.len() as u64));
        response.body = Body::Fixed(self.data);
        Ok(response)
    }
}
