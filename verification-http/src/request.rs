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
use crate::auth::AuthToken;
use crate::error::ConjureVerificationError;
use crate::headers;
use crate::SerializableFormat;
use conjure_verification_error::{Code, Error, Result};
use http::header::HeaderMap;
use mime::{Mime, STAR};
use serde::de::DeserializeOwned;
use serde_json;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::io::Read;
use std::str::FromStr;

const BODY_SIZE_LIMIT_BYTES: u64 = 1024 * 1024;

pub struct Request<'a> {
    path_params: &'a HashMap<String, String>,
    query_params: &'a HashMap<String, Vec<String>>,
    headers: &'a HeaderMap,
    body: &'a mut dyn Read,
    body_size_limit: u64,
}

impl<'a> Request<'a> {
    pub fn new(
        path_params: &'a HashMap<String, String>,
        query_params: &'a HashMap<String, Vec<String>>,
        headers: &'a HeaderMap,
        body: &'a mut dyn Read,
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
            })
            .collect()
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
        let mime = match headers::get_content_type(self.headers) {
            Ok(Some(content_type)) => content_type,
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

        let (is_io, error): (bool, Box<dyn StdError + Sync + Send>) = match format {
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

    pub fn raw_body(&mut self) -> &mut dyn Read {
        &mut self.body
    }

    pub fn headers(&self) -> &HeaderMap {
        self.headers
    }

    pub fn response_format<'f, T>(&self, formats: &'f [T]) -> Result<&'f T>
    where
        T: Format,
    {
        let accept = match headers::get_accept(self.headers) {
            Ok(Some(accept)) => accept,
            Ok(None) => return Ok(&formats[0]),
            Err(e) => return Err(Error::new(e, Code::InvalidArgument)),
        };

        match content_type(&accept, formats) {
            Some(ty) => Ok(ty),
            None => Err(Error::new_safe(
                "unable to select a response type",
                ConjureVerificationError::NotAcceptable,
            )),
        }
    }

    pub fn auth_token(&self) -> Result<AuthToken> {
        match headers::get_bearer_token(self.headers).ok().and_then(|t| t) {
            Some(token) => Ok(AuthToken::new(&token)),
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

fn content_type<'a, T>(accept: &[headers::AcceptEntry], types: &'a [T]) -> Option<&'a T>
where
    T: Format,
{
    let mut accept = accept.to_vec();
    accept.sort_by(quality_order);

    for type_ in types.iter() {
        // we sorted ascending so iterate backwards
        for accept in accept.iter().rev() {
            if type_.matches(&accept.mime) {
                return Some(type_);
            }
        }
    }

    None
}

// Order by quality and then "specificity"
fn quality_order(a: &headers::AcceptEntry, b: &headers::AcceptEntry) -> Ordering {
    match a.quality.cmp(&b.quality) {
        Ordering::Equal => {}
        o => return o,
    }

    match (a.mime.type_(), b.mime.type_()) {
        (STAR, STAR) => {}
        (STAR, _) => return Ordering::Less,
        (_, STAR) => return Ordering::Greater,
        _ => {}
    }

    match (a.mime.subtype(), b.mime.subtype()) {
        (STAR, STAR) => {}
        (STAR, _) => return Ordering::Less,
        (_, STAR) => return Ordering::Greater,
        _ => {}
    }

    // This is weird and bad
    a.mime.params().count().cmp(&b.mime.params().count())
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
        headers::set_content_type(&mut headers, &APPLICATION_JSON);

        let query_params = HashMap::new();
        let path_params = HashMap::new();

        let body_length = json.len() as u64 + 1;
        let mut request = Request::new(&path_params, &query_params, &headers, &mut json);
        request.body_size_limit = body_length;

        let actual = request.body::<Vec<u32>>().unwrap();
        assert_eq!(body, actual);
    }

    #[test]
    fn large_body() {
        let body = (0..100).collect::<Vec<_>>();
        let json = serde_json::to_vec(&body).unwrap();
        let mut json = &json[..];

        let mut headers = HeaderMap::new();
        headers::set_content_type(&mut headers, &APPLICATION_JSON);

        let query_params = HashMap::new();
        let path_params = HashMap::new();

        let body_length = json.len() as u64 - 1;
        let mut request = Request::new(&path_params, &query_params, &headers, &mut json);
        request.body_size_limit = body_length;

        assert!(request.body::<Vec<u32>>().is_err());
    }
}
