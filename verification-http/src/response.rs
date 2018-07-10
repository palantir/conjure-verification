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
use conjure_verification_error::{Error, Result};
use error::ConjureVerificationError;
use http::header::{HeaderMap, HeaderValue};
use http::StatusCode;
use serde::Serialize;
use serde_json;
use std::io::Write;
use typed_headers::{ContentLength, ContentType, HeaderMapExt};

use request::{Format, Request};
use SerializableFormat;

pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Body,
}

impl Response {
    pub fn new(status: StatusCode) -> Response {
        Response {
            status,
            headers: HeaderMap::new(),
            body: Body::Empty,
        }
    }
}

pub enum Body {
    Empty,
    Fixed(Bytes),
    Streaming(Box<WriteBody>),
}

pub trait WriteBody {
    fn write_body(&mut self, w: &mut Write) -> Result<()>;
}

pub trait IntoResponse {
    fn into_response(self, request: &Request) -> Result<Response>;
}

impl IntoResponse for Response {
    fn into_response(self, _: &Request) -> Result<Response> {
        Ok(self)
    }
}

impl<T> IntoResponse for T
where
    T: Serialize,
{
    fn into_response(self, request: &Request) -> Result<Response> {
        let format = *request.response_format(&[SerializableFormat::Json])?;

        let buf = match format {
            SerializableFormat::Json => serde_json::to_vec(&self).map_err(Error::internal)?,
        };

        let mut response = Response::new(StatusCode::OK);
        response
            .headers
            .typed_insert(&ContentType(format.mime().clone()));
        response
            .headers
            .typed_insert(&ContentLength(buf.len() as u64));
        response
            .headers
            .append("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
        response.body = Body::Fixed(Bytes::from(buf));
        Ok(response)
    }
}

pub struct StreamedSerializable<T>(pub T);

impl<T> IntoResponse for StreamedSerializable<T>
where
    T: 'static + Serialize,
{
    fn into_response(self, request: &Request) -> Result<Response> {
        let format = *request.response_format(&[SerializableFormat::Json])?;

        let mut response = Response::new(StatusCode::OK);
        response
            .headers
            .typed_insert(&ContentType(format.mime().clone()));
        response
            .headers
            .append("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
        response.body = Body::Streaming(Box::new(SerializableBody {
            format,
            body: self.0,
        }));
        Ok(response)
    }
}

struct SerializableBody<T> {
    format: SerializableFormat,
    body: T,
}

impl<T> WriteBody for SerializableBody<T>
where
    T: Serialize,
{
    fn write_body(&mut self, res: &mut Write) -> Result<()> {
        match self.format {
            SerializableFormat::Json => serde_json::to_writer(res, &self.body).map_err(|e| {
                if e.is_io() {
                    Error::new_safe(e, ConjureVerificationError::ClientIo)
                } else {
                    Error::internal(e)
                }
            }),
        }
    }
}

pub struct NoContent;

impl IntoResponse for NoContent {
    fn into_response(self, _: &Request) -> Result<Response> {
        let mut response = Response::new(StatusCode::NO_CONTENT);
        response
            .headers
            .append("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
        Ok(response)
    }
}
