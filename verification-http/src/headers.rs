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

//! Minimal typed header helpers replacing the (unmaintained, http-0.1-only)
//! `typed-headers` crate.

use base64::Engine;
use http::header::{HeaderMap, HeaderValue};
use mime::Mime;
use std::time::SystemTime;

pub type HeaderResult<T> = Result<T, String>;

fn to_str(value: &HeaderValue) -> HeaderResult<&str> {
    value
        .to_str()
        .map_err(|e| format!("invalid header value: {}", e))
}

/// Reads and parses the `Content-Type` header as a MIME type.
pub fn get_content_type(headers: &HeaderMap) -> HeaderResult<Option<Mime>> {
    match headers.get(http::header::CONTENT_TYPE) {
        Some(value) => to_str(value)?
            .parse::<Mime>()
            .map(Some)
            .map_err(|e| format!("invalid Content-Type header: {}", e)),
        None => Ok(None),
    }
}

/// Sets the `Content-Type` header to a MIME type.
pub fn set_content_type(headers: &mut HeaderMap, mime: &Mime) {
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).expect("mime is a valid header value"),
    );
}

/// Reads the `Content-Length` header.
pub fn get_content_length(headers: &HeaderMap) -> HeaderResult<Option<u64>> {
    match headers.get(http::header::CONTENT_LENGTH) {
        Some(value) => to_str(value)?
            .parse::<u64>()
            .map(Some)
            .map_err(|e| format!("invalid Content-Length header: {}", e)),
        None => Ok(None),
    }
}

/// Sets the `Content-Length` header.
pub fn set_content_length(headers: &mut HeaderMap, length: u64) {
    headers.insert(
        http::header::CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string()).unwrap(),
    );
}

/// Reads the `Authorization` header, returning the bearer token if present.
pub fn get_bearer_token(headers: &HeaderMap) -> HeaderResult<Option<String>> {
    match headers.get(http::header::AUTHORIZATION) {
        Some(value) => {
            let value = to_str(value)?;
            if value.len() > 7 && value[..7].eq_ignore_ascii_case("bearer ") {
                Ok(Some(value[7..].to_string()))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}

/// Sets the `Authorization` header to a bearer token.
pub fn set_bearer_token(headers: &mut HeaderMap, token: &str) {
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token)).expect("invalid bearer token"),
    );
}

/// A single entry of an `Accept` header, with its quality value.
#[derive(Debug, Clone)]
pub struct AcceptEntry {
    pub mime: Mime,
    pub quality: u16,
}

impl AcceptEntry {
    fn parse(s: &str) -> HeaderResult<AcceptEntry> {
        let mime = s
            .parse::<Mime>()
            .map_err(|e| format!("invalid Accept entry `{}`: {}", s, e))?;
        let quality = match mime.get_param("q") {
            Some(q) => {
                let q = q.as_ref();
                let mut parts = q.splitn(2, '.');
                let whole: u16 = parts
                    .next()
                    .and_then(|p| p.parse().ok())
                    .ok_or_else(|| format!("invalid quality value `{}`", q))?;
                let frac = parts.next().unwrap_or("");
                let frac: u16 = format!("{:0<3}", &frac[..frac.len().min(3)])
                    .parse()
                    .map_err(|_| format!("invalid quality value `{}`", q))?;
                whole
                    .checked_mul(1000)
                    .and_then(|w| w.checked_add(frac))
                    .ok_or_else(|| format!("invalid quality value `{}`", q))?
            }
            None => 1000,
        };
        Ok(AcceptEntry { mime, quality })
    }
}

/// Reads the `Accept` header. Absent header yields an empty list.
pub fn get_accept(headers: &HeaderMap) -> HeaderResult<Option<Vec<AcceptEntry>>> {
    match headers.get(http::header::ACCEPT) {
        Some(value) => to_str(value)?
            .split(',')
            .map(|entry| AcceptEntry::parse(entry.trim()))
            .collect::<HeaderResult<Vec<_>>>()
            .map(Some),
        None => Ok(None),
    }
}

/// The value of a `Content-Encoding` header.
#[derive(Debug, PartialEq, Eq)]
pub enum Encoding {
    Identity,
    Gzip,
    Deflate,
    Other(String),
}

/// Reads the `Content-Encoding` header as a list of codings, in order.
pub fn get_content_encoding(headers: &HeaderMap) -> HeaderResult<Vec<Encoding>> {
    match headers.get(http::header::CONTENT_ENCODING) {
        Some(value) => to_str(value)?
            .split(',')
            .map(|coding| {
                Ok(match coding.trim().to_ascii_lowercase().as_str() {
                    "identity" => Encoding::Identity,
                    "gzip" => Encoding::Gzip,
                    "deflate" => Encoding::Deflate,
                    other => Encoding::Other(other.to_string()),
                })
            })
            .collect(),
        None => Ok(vec![]),
    }
}

/// Sets the `Allow` header from a list of HTTP methods.
pub fn set_allow(headers: &mut HeaderMap, methods: &[http::Method]) {
    let value = methods
        .iter()
        .map(|m| m.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    headers.insert(
        http::header::ALLOW,
        HeaderValue::from_str(&value).expect("methods are valid header values"),
    );
}

/// The parsed value of a `Retry-After` header.
#[derive(Debug, PartialEq, Eq)]
pub enum RetryAfter {
    DelaySeconds(u64),
    HttpDate(SystemTime),
}

/// Reads the `Retry-After` header.
pub fn get_retry_after(headers: &HeaderMap) -> HeaderResult<Option<RetryAfter>> {
    let value = match headers.get(http::header::RETRY_AFTER) {
        Some(value) => to_str(value)?.trim(),
        None => return Ok(None),
    };

    if let Ok(seconds) = value.parse::<u64>() {
        return Ok(Some(RetryAfter::DelaySeconds(seconds)));
    }

    // HTTP-date, e.g. `Wed, 21 Oct 2015 07:28:00 GMT`
    if let Ok(date) = httpdate::parse_http_date(value) {
        return Ok(Some(RetryAfter::HttpDate(date)));
    }

    Err(format!("invalid Retry-After header `{}`", value))
}

/// Sets the `Host` header.
pub fn set_host(headers: &mut HeaderMap, host: &str, port: Option<u16>) {
    let value = match port {
        Some(port) => format!("{}:{}", host, port),
        None => host.to_string(),
    };
    headers.insert(
        http::header::HOST,
        HeaderValue::from_str(&value).expect("invalid host"),
    );
}

/// Sets the `Proxy-Authorization` header to basic credentials.
pub fn set_proxy_authorization_basic(headers: &mut HeaderMap, username: &str, password: &str) {
    let value = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username, password))
    );
    headers.insert(
        http::header::PROXY_AUTHORIZATION,
        HeaderValue::from_str(&value).expect("invalid credentials"),
    );
}

/// Reads the `Proxy-Authorization` header's basic credentials.
pub fn get_proxy_authorization_basic(
    headers: &HeaderMap,
) -> HeaderResult<Option<(String, String)>> {
    match headers.get(http::header::PROXY_AUTHORIZATION) {
        Some(value) => {
            let value = to_str(value)?;
            if value.len() > 6 && value[..6].eq_ignore_ascii_case("basic ") {
                let decoded = base64::engine::general_purpose::STANDARD.decode(value[6..].trim())
                    .map_err(|e| format!("invalid Proxy-Authorization header: {}", e))?;
                let decoded = String::from_utf8(decoded)
                    .map_err(|e| format!("invalid Proxy-Authorization header: {}", e))?;
                let mut parts = decoded.splitn(2, ':');
                let username = parts.next().unwrap_or("").to_string();
                let password = parts.next().unwrap_or("").to_string();
                Ok(Some((username, password)))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}
