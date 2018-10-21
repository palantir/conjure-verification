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
use auth::AuthToken;
use conjure_verification_error::{Code, Error, Result};
use error::ConjureVerificationError;
use http::header::HeaderMap;
use mime::{Mime, STAR};
use serde::de::DeserializeOwned;
use serde_json;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::io::Read;
use std::str::FromStr;
use typed_headers::{Accept, Authorization, ContentType, HeaderMapExt, QualityItem};

use SerializableFormat;

const BODY_SIZE_LIMIT_BYTES: u64 = 1024 * 1024;

pub struct Request<'a> {
    path_params: &'a HashMap<String, String>,
    query_params: &'a HashMap<String, Vec<String>>,
    headers: &'a HeaderMap,
    body: &'a mut Read,
    body_size_limit: u64,
}

impl<'a> Request<'a> {
    pub fn new(
        path_params: &'a HashMap<String, String>,
        query_params: &'a HashMap<String, Vec<String>>,
        headers: &'a HeaderMap,
        body: &'a mut Read,
    ) -> Request<'a> {
        Request {
            path_params,
            query_params,
            headers,
            body,
            body_size_limit: BODY_SIZE_LIMIT_BYTES,
        }
    }

    pub fn path_param(&self, name: &str) -> &str {
        self.path_params.get(name).expect("invalid path param")
    }

    pub fn multi_query_param<T>(&self, name: &str) -> Result<Vec<T>>
    where
        T: FromStr,
        T::Err: 'static + StdError + Sync + Send,
    {
        self.query_params
            .get(name)
            .into_iter()
            .flat_map(|v| v)
            .map(|v| {
                v.parse::<T>().map_err(|e| {
                    Error::new_safe(
                        e,
                        ConjureVerificationError::InvalidQueryParameter {
                            parameter: name.to_string(),
                        },
                    )
                })
            }).collect()
    }

    pub fn query_param<T>(&self, name: &str) -> Result<T>
    where
        T: FromStr,
        T::Err: 'static + StdError + Sync + Send,
    {
        match self.opt_query_param(name) {
            Ok(Some(v)) => Ok(v),
            Ok(None) => Err(Error::new_safe(
                "missing query parameter",
                ConjureVerificationError::MissingQueryParameter {
                    parameter: name.to_string(),
                },
            )),
            Err(e) => Err(e),
        }
    }

    pub fn opt_query_param<T>(&self, name: &str) -> Result<Option<T>>
    where
        T: FromStr,
        T::Err: 'static + StdError + Sync + Send,
    {
        let param = match self.query_params.get(name) {
            Some(params) if params.len() == 1 => &params[0],
            Some(_) => {
                return Err(Error::new_safe(
                    "duplicate query parameter",
                    ConjureVerificationError::DuplicateQueryParameter {
                        parameter: name.to_string(),
                    },
                ))
            }
            None => return Ok(None),
        };

        match param.parse() {
            Ok(v) => Ok(Some(v)),
            Err(e) => Err(Error::new_safe(
                e,
                ConjureVerificationError::InvalidQueryParameter {
                    parameter: name.to_string(),
                },
            )),
        }
    }

    pub fn query_params(&self) -> &HashMap<String, Vec<String>> {
        &self.query_params
    }

    pub fn body<T>(&mut self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mime = match self.headers.typed_get::<ContentType>() {
            Ok(Some(content_type)) => content_type.0,
            Ok(None) => {
                return Err(Error::new_safe(
                    "missing content type",
                    ConjureVerificationError::UnsupportedContentType,
                ));
            }
            Err(e) => return Err(Error::new_safe(e, Code::InvalidArgument)),
        };

        let format = if SerializableFormat::Json.matches(&mime) {
            SerializableFormat::Json
        } else {
            return Err(Error::new_safe(
                "unsupported content type",
                ConjureVerificationError::UnsupportedContentType,
            ));
        };

        let mut reader = self.body.take(self.body_size_limit);

        let (is_io, error): (bool, Box<StdError + Sync + Send>) = match format {
            SerializableFormat::Json => match serde_json::from_reader(&mut reader) {
                Ok(t) => return Ok(t),
                Err(e) => (e.is_io(), Box::new(e)),
            },
        };

        // this could be technically incorrect if the deserialization hits some other error after reading exactly 50MB,
        // but that's not a super interesting edge case.
        let code = if reader.limit() == 0 {
            ConjureVerificationError::RequestEntityTooLarge
        } else if is_io {
            ConjureVerificationError::ClientIo
        } else {
            ConjureVerificationError::InvalidRequestBody
        };
        Err(Error::new(error, code))
    }

    pub fn raw_body(&mut self) -> &mut Read {
        &mut self.body
    }

    pub fn headers(&self) -> &HeaderMap {
        self.headers
    }

    pub fn response_format<'f, T>(&self, formats: &'f [T]) -> Result<&'f T>
    where
        T: Format,
    {
        let accept = match self.headers.typed_get::<Accept>() {
            Ok(Some(accept)) => accept,
            Ok(None) => return Ok(&formats[0]),
            Err(e) => return Err(Error::new(e, Code::InvalidArgument)),
        };

        match content_type(&accept, formats) {
            Some(ty) => Ok(ty),
            None => {
                return Err(Error::new_safe(
                    "unable to select a response type",
                    ConjureVerificationError::NotAcceptable,
                ))
            }
        }
    }

    pub fn auth_token(&self) -> Result<AuthToken> {
        let header = self.headers.typed_get::<Authorization>();

        match header
            .as_ref()
            .ok()
            .and_then(|h| h.as_ref())
            .and_then(|h| h.as_bearer())
        {
            Some(token) => Ok(AuthToken::new(token.as_str())),
            None => Err(Error::new_safe(
                "auth token not provided",
                ConjureVerificationError::MissingAuthToken,
            )),
        }
    }
}

pub trait Format {
    fn mime(&self) -> &Mime;

    fn matches(&self, other: &Mime) -> bool {
        let mime = self.mime();

        if other.type_() != STAR && other.type_() != mime.type_() {
            return false;
        }

        if other.subtype() != STAR && other.subtype() != mime.subtype() {
            return false;
        }

        for (name, value) in other.params() {
            if mime
                .get_param(name)
                .map(|value2| value != value2)
                .unwrap_or(false)
            {
                return false;
            }
        }

        true
    }
}

fn content_type<'a, T>(accept: &Accept, types: &'a [T]) -> Option<&'a T>
where
    T: Format,
{
    let mut accept = accept.0.clone();
    accept.sort_by(quality_order);

    for type_ in types.iter() {
        // we sorted ascending so iterate backwards
        for accept in accept.iter().rev() {
            if type_.matches(&accept.item) {
                return Some(type_);
            }
        }
    }

    None
}

// Order by quality and then "specificity"
fn quality_order(a: &QualityItem<Mime>, b: &QualityItem<Mime>) -> Ordering {
    match a.quality.cmp(&b.quality) {
        Ordering::Equal => {}
        o => return o,
    }

    match (a.item.type_(), b.item.type_()) {
        (STAR, STAR) => {}
        (STAR, _) => return Ordering::Less,
        (_, STAR) => return Ordering::Greater,
        _ => {}
    }

    match (a.item.subtype(), b.item.subtype()) {
        (STAR, STAR) => {}
        (STAR, _) => return Ordering::Less,
        (_, STAR) => return Ordering::Greater,
        _ => {}
    }

    // This is weird and bad
    a.item.params().count().cmp(&b.item.params().count())
}

#[cfg(test)]
mod test {
    use mime::APPLICATION_JSON;

    use super::*;

    #[test]
    fn small_body() {
        let body = (0..100).collect::<Vec<_>>();
        let json = serde_json::to_vec(&body).unwrap();
        let mut json = &json[..];

        let mut headers = HeaderMap::new();
        headers.typed_insert(&ContentType(APPLICATION_JSON));

        let query_params = HashMap::new();
        let path_params = HashMap::new();

        let mut request = Request::new(&path_params, &query_params, &headers, &mut json);
        request.body_size_limit = json.len() as u64 + 1;

        let actual = request.body::<Vec<u32>>().unwrap();
        assert_eq!(body, actual);
    }

    #[test]
    fn large_body() {
        let body = (0..100).collect::<Vec<_>>();
        let json = serde_json::to_vec(&body).unwrap();
        let mut json = &json[..];

        let mut headers = HeaderMap::new();
        headers.typed_insert(&ContentType(APPLICATION_JSON));

        let query_params = HashMap::new();
        let path_params = HashMap::new();

        let mut request = Request::new(&path_params, &query_params, &headers, &mut json);
        request.body_size_limit = json.len() as u64 - 1;

        assert!(request.body::<Vec<u32>>().is_err());
    }
}
