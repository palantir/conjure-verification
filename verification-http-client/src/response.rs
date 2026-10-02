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

use crate::errors::{Error, Result};
use bytes::Bytes;
use conjure_verification_http::headers::{self, Encoding};
use flate2::bufread::{GzDecoder, ZlibDecoder};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::{self, HeaderMap, StatusCode};
use mime;
use mime::Mime;
use serde::de::DeserializeOwned;
use serde_cbor;
use serde_json;
use serde_urlencoded;
use std::io::{self, BufRead, BufReader, Cursor, Read};

use crate::{RemoteError, APPLICATION_CBOR, RUNTIME};

/// An HTTP response.
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: IdentityBody,
}

impl Response {
    pub(crate) fn new(response: hyper::Response<Incoming>) -> Response {
        let (parts, body) = response.into_parts();
        Response {
            status: parts.status,
            headers: parts.headers,
            body: IdentityBody {
                body,
                cur: Cursor::new(Bytes::new()),
            },
        }
    }

    /// Returns the request status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the response's headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    fn format(&self) -> Result<Format> {
        let content_type =
            headers::get_content_type(&self.headers).map_err(Error::internal_safe)?;
        Format::new(content_type)
    }

    pub(crate) fn into_error(self) -> Error {
        let status = self.status;
        let format = match self.format() {
            Ok(format) => Some(format),
            Err(e) => {
                info!("unable to determine error response format: {}", e);
                None
            }
        };

        let body = match self.raw_body() {
            Ok(body) => {
                let mut buf = vec![];
                // limit how much we read in case something weird's going on
                if let Err(e) = body.take(10 * 1024).read_to_end(&mut buf) {
                    info!("error reading response body: {}", Error::internal_safe(e));
                }
                buf
            }
            Err(e) => {
                info!("unable to decode body: {}", e);
                vec![]
            }
        };

        let error = RemoteError {
            status,
            error: format.and_then(|f| f.deserialize(&mut &*body).ok()),
        };
        let log_body = error.error.is_none();
        let mut error = Error::internal_safe(error);
        if log_body {
            error = error.with_unsafe_param("body", String::from_utf8_lossy(&body));
        }

        error
    }

    /// Deserializes the response body.
    ///
    /// `application/json`, `application/cbor`, and `application/x-www-form-urlencoded` body types are currently
    /// supported.
    pub fn body<T>(self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let format = self.format()?;
        format.deserialize(&mut self.raw_body()?.0)
    }

    /// Returns a reader of the raw response body.
    pub fn raw_body(self) -> Result<ResponseBody> {
        let encoding =
            headers::get_content_encoding(&self.headers).map_err(Error::internal_safe)?;

        let body: Box<dyn BufRead> = match encoding.as_slice() {
            [] | [Encoding::Identity] => Box::new(self.body),
            [Encoding::Gzip] => Box::new(BufReader::new(GzDecoder::new(self.body))),
            [Encoding::Deflate] => Box::new(BufReader::new(ZlibDecoder::new(self.body))),
            v => {
                return Err(Error::internal_safe("unsupported Content-Encoding")
                    .with_safe_param("encoding", format!("{:?}", v)))
            }
        };

        Ok(ResponseBody(body))
    }
}

enum Format {
    Json,
    Cbor,
    Urlencoded,
    OctetStream,
}

impl Format {
    fn new(content_type: Option<Mime>) -> Result<Format> {
        match content_type {
            Some(ref v) if *v == mime::APPLICATION_JSON => Ok(Format::Json),
            Some(ref v) if *v == *APPLICATION_CBOR => Ok(Format::Cbor),
            Some(ref v) if *v == mime::APPLICATION_WWW_FORM_URLENCODED => Ok(Format::Urlencoded),
            Some(ref v) if *v == mime::APPLICATION_OCTET_STREAM => Ok(Format::OctetStream),
            Some(v) => Err(Error::internal_safe("unsupported Content-Type")
                .with_safe_param("type", format!("{:?}", v))),
            None => Err(Error::internal_safe("Content-Type header missing")),
        }
    }

    fn deserialize<T>(&self, r: &mut dyn Read) -> Result<T>
    where
        T: DeserializeOwned,
    {
        match *self {
            Format::Json => serde_json::from_reader(r).map_err(Error::internal),
            Format::Cbor => serde_cbor::from_reader(r).map_err(Error::internal),
            Format::Urlencoded => serde_urlencoded::from_reader(r).map_err(Error::internal),
            Format::OctetStream => Err(Error::internal_safe("Can't deserialize octet_stream body")),
        }
    }
}

pub struct ResponseBody(pub Box<dyn BufRead>);

impl Read for ResponseBody {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

struct IdentityBody {
    body: Incoming,
    cur: Cursor<Bytes>,
}

impl Read for IdentityBody {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let nread = {
            let read_buf = self.fill_buf()?;
            let nread = usize::min(buf.len(), read_buf.len());
            buf[..nread].copy_from_slice(&read_buf[..nread]);
            nread
        };
        self.consume(nread);
        Ok(nread)
    }
}

impl BufRead for IdentityBody {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        // Read the body a frame at a time so that errors (e.g. a truncated body) are reported to the reader.
        while self.cur.position() == self.cur.get_ref().len() as u64 {
            match RUNTIME.block_on(self.body.frame()) {
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        self.cur = Cursor::new(data);
                    }
                }
                Some(Err(e)) => return Err(io::Error::new(io::ErrorKind::Other, e)),
                None => break,
            }
        }

        self.cur.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.cur.consume(amt)
    }
}
