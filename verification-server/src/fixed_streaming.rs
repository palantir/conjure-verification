// (c) Copyright 2019 Palantir Technologies Inc. All rights reserved.
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
use conjure_verification_error::{Error, Result};
use conjure_verification_http::request::Request;
use conjure_verification_http::response::IntoResponse;
use conjure_verification_http::response::WriteBody;
use conjure_verification_http::response::{Body, Response};
use http::header::HeaderValue;
use http::StatusCode;
use itertools::FoldWhile::{Continue, Done};
use itertools::Itertools;
use mime::APPLICATION_OCTET_STREAM;
use std::io::Write;
use std::thread;
use std::time;
use typed_headers::{ContentType, HeaderMapExt};

pub struct StreamingResponse {
    pub data: Vec<u8>,
}

impl WriteBody for StreamingResponse {
    fn write_body(&mut self, w: &mut dyn Write) -> Result<()> {
        return self
            .data
            .chunks(1024)
            .fold_while(Ok(()), |_res, chunk| {
                thread::sleep(time::Duration::from_millis(10));
                let chunk_res = w.write_all(chunk)
                    .and(w.flush())
                    .map_err(|e| Error::internal(e));
                return if chunk_res.is_ok() {
                    Continue(chunk_res)
                } else {
                    Done(chunk_res)
                };
            }).into_inner();
    }
}

impl IntoResponse for StreamingResponse {
    fn into_response(self, _request: &Request) -> Result<Response> {
        let mut response = Response::new(StatusCode::OK);
        response
            .headers
            .typed_insert(&ContentType(APPLICATION_OCTET_STREAM));
        response
            .headers
            .append("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
        response.body = Body::Streaming(Box::new(self));
        Ok(response)
    }
}
