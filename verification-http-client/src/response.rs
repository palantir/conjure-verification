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

use errors::{Error, Result};
use flate2::bufread::{GzDecoder, ZlibDecoder};
use futures::stream::{self, Stream};
use hyper::{self, Body, HeaderMap, StatusCode};
use mime;
use serde::de::DeserializeOwned;
use serde_cbor;
use serde_json;
use serde_urlencoded;
use std::io::{self, BufRead, BufReader, Cursor, Read};
use typed_headers::{ContentCoding, ContentEncoding, ContentType, HeaderMapExt};

use {RemoteError, APPLICATION_CBOR};

/// An HTTP response.
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: IdentityBody,
}

impl Response {
    pub(crate) fn new(response: hyper::Response<Body>) -> Response {
        let (parts, body) = response.into_parts();
        Response {
            status: parts.status,
            headers: parts.headers,
            body: IdentityBody {
                it: body.wait(),
                cur: Cursor::new(hyper::Chunk::from("")),
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
        let content_type = self
            .headers
            .typed_get::<ContentType>()
            .map_err(Error::internal_safe)?;
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
        let encoding = self
            .headers
            .typed_get::<ContentEncoding>()
            .map_err(Error::internal_safe)?;

        let body: Box<BufRead> = match encoding.as_ref().map(|c| &***c) {
            None | Some([ContentCoding::IDENTITY]) => Box::new(self.body),
            Some([ContentCoding::GZIP]) => Box::new(BufReader::new(GzDecoder::new(self.body))),
            Some([ContentCoding::DEFLATE]) => Box::new(BufReader::new(ZlibDecoder::new(self.body))),
            Some(v) => {
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
    Octet_stream,
}

impl Format {
    fn new(content_type: Option<ContentType>) -> Result<Format> {
        match content_type {
            Some(ref v) if v.0 == mime::APPLICATION_JSON => Ok(Format::Json),
            Some(ref v) if v.0 == *APPLICATION_CBOR => Ok(Format::Cbor),
            Some(ref v) if v.0 == mime::APPLICATION_WWW_FORM_URLENCODED => Ok(Format::Urlencoded),
            Some(ref v) if v.0 == mime::APPLICATION_OCTET_STREAM => Ok(Format::Octet_stream),
            Some(v) => Err(Error::internal_safe("unsupported Content-Type")
                .with_safe_param("type", format!("{:?}", v))),
            None => Err(Error::internal_safe("Content-Type header missing")),
        }
    }

    fn deserialize<T>(&self, r: &mut Read) -> Result<T>
    where
        T: DeserializeOwned,
    {
        match *self {
            Format::Json => serde_json::from_reader(r).map_err(Error::internal),
            Format::Cbor => serde_cbor::from_reader(r).map_err(Error::internal),
            Format::Urlencoded => serde_urlencoded::from_reader(r).map_err(Error::internal),
            Format::Octet_stream => Err(Error::internal_safe("Can't deserialize octet_stream body"))
        }
    }
}

pub struct ResponseBody(pub Box<BufRead>);

impl Read for ResponseBody {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

struct IdentityBody {
    it: stream::Wait<hyper::Body>,
    cur: Cursor<hyper::Chunk>,
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
        while self.cur.position() == self.cur.get_ref().len() as u64 {
            match self.it.next() {
                Some(Ok(chunk)) => self.cur = Cursor::new(chunk),
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
