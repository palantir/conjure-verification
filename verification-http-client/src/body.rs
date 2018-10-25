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
use errors::{Error, Result};
use mime::{self, Mime};
use serde::Serialize;
use serde_cbor;
use serde_json;
use serde_urlencoded;
use std::io::Write;

use APPLICATION_CBOR;

/// A request body.
pub trait Body {
    /// Returns the length of the body if known.
    fn content_length(&self) -> Option<u64>;

    /// Returns the content type of the body.
    fn content_type(&self) -> Mime;

    /// Returns the entire body if it is fully buffered.
    ///
    /// `write` will only be called if this method returns `None`.
    fn full_body(&self) -> Option<Bytes> {
        None
    }

    /// Writes the body data out.
    fn write(&mut self, w: &mut Write) -> Result<()>;

    /// Resets the body to its start.
    ///
    /// Returns `true` iff the body was successfully reset.
    ///
    /// Requests with non-resettable bodies cannot be retried.
    fn reset(&mut self) -> bool;
}

/// A trait implemented by types which can be converted to request `Body`s.
pub trait IntoBody {
    type Target: Body;

    fn into_body(self) -> Result<Self::Target>;
}

impl<T> IntoBody for T
where
    T: Body,
{
    type Target = Self;

    fn into_body(self) -> Result<Self> {
        Ok(self)
    }
}

/// A `Body` type which serializes a value as CBOR.
pub struct Cbor<T>(pub T);

impl<T> IntoBody for Cbor<T>
where
    T: Serialize,
{
    type Target = BytesBody;

    fn into_body(self) -> Result<BytesBody> {
        let body = serde_cbor::to_vec(&self.0).map_err(|e| Error::internal(e))?;
        Ok(BytesBody::new(body, APPLICATION_CBOR.clone()))
    }
}

/// A `Body` type which serializes a value as JSON.
pub struct Json<T>(pub T);

impl<T> IntoBody for Json<T>
where
    T: Serialize,
{
    type Target = BytesBody;

    fn into_body(self) -> Result<BytesBody> {
        let body = serde_json::to_vec(&self.0).map_err(|e| Error::internal(e))?;
        Ok(BytesBody::new(body, mime::APPLICATION_JSON))
    }
}

/// A `Body` type which serializes a value as `application/x-www-form-urlencoded`.
pub struct Urlencoded<T>(pub T);

impl<T> IntoBody for Urlencoded<T>
where
    T: Serialize,
{
    type Target = BytesBody;

    fn into_body(self) -> Result<BytesBody> {
        let body = serde_urlencoded::to_string(&self.0).map_err(|e| Error::internal(e))?;
        Ok(BytesBody::new(
            body.into_bytes(),
            mime::APPLICATION_WWW_FORM_URLENCODED,
        ))
    }
}

/// A simple type implementing `Body` which constists of a byte buffer and a
/// MIME type.
///
/// It reports its content length and is resetable.
pub struct BytesBody {
    body: Bytes,
    mime: Mime,
}

impl BytesBody {
    pub fn new<T>(body: T, mime: Mime) -> BytesBody
    where
        T: Into<Bytes>,
    {
        BytesBody {
            body: body.into(),
            mime,
        }
    }
}

impl Body for BytesBody {
    fn content_length(&self) -> Option<u64> {
        Some(self.body.len() as u64)
    }

    fn content_type(&self) -> Mime {
        self.mime.clone()
    }

    fn full_body(&self) -> Option<Bytes> {
        Some(self.body.clone())
    }

    fn write(&mut self, _: &mut Write) -> Result<()> {
        unreachable!()
    }

    fn reset(&mut self) -> bool {
        true
    }
}
