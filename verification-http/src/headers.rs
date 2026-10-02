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
use http::header::{HeaderMap, HeaderName, HeaderValue};
use mime::Mime;
use std::str::FromStr;
use std::time::SystemTime;

pub type HeaderResult<T> = Result<T, String>;

fn to_str(value: &HeaderValue) -> HeaderResult<&str> {
    value
        .to_str()
        .map_err(|e| format!("invalid header value: {}", e))
}

// The parsing below mirrors the `typed-headers` 0.1 crate this module replaces, so the verification tools accept
// and reject the same header values as before.

const TOO_MANY_VALUES: &str = "too many header values";
const TOO_FEW_VALUES: &str = "too few header values";
const INVALID_VALUE: &str = "invalid header value";

/// Parses a header which may appear at most once.
fn parse_single_value<T>(headers: &HeaderMap, name: HeaderName) -> HeaderResult<Option<T>>
where
    T: FromStr,
{
    let mut values = headers.get_all(name).iter();
    let value = match values.next() {
        Some(value) => to_str(value)?
            .trim()
            .parse()
            .map_err(|_| INVALID_VALUE.to_string())?,
        None => return Ok(None),
    };
    match values.next() {
        Some(_) => Err(TOO_MANY_VALUES.to_string()),
        None => Ok(Some(value)),
    }
}

/// Parses a comma-delimited list header, merging all occurrences of the header and skipping empty elements.
fn parse_comma_delimited<T>(headers: &HeaderMap, name: HeaderName) -> HeaderResult<Option<Vec<T>>>
where
    T: FromStr,
{
    let mut out = vec![];
    let mut empty = true;
    for value in headers.get_all(name) {
        empty = false;
        for elem in to_str(value)?.split(',') {
            let elem = elem.trim();
            if elem.is_empty() {
                continue;
            }
            out.push(elem.parse().map_err(|_| INVALID_VALUE.to_string())?);
        }
    }

    if empty {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}

fn is_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| {
            matches!(b,
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+'
                | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~')
        })
}

/// Reads and parses the `Content-Type` header as a MIME type.
pub fn get_content_type(headers: &HeaderMap) -> HeaderResult<Option<Mime>> {
    parse_single_value(headers, http::header::CONTENT_TYPE)
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

/// A single entry of an `Accept` header, with its quality value (0 to 1000).
#[derive(Debug, Clone)]
pub struct AcceptEntry {
    pub mime: Mime,
    pub quality: u16,
}

impl FromStr for AcceptEntry {
    type Err = mime::FromStrError;

    fn from_str(mut s: &str) -> Result<AcceptEntry, mime::FromStrError> {
        // A malformed weight is not an error: it is left in place and parsed as a MIME parameter.
        let quality = match WeightParser::parse(s) {
            Some((remaining, quality)) => {
                s = &s[..remaining];
                quality
            }
            None => 1000,
        };
        Ok(AcceptEntry {
            mime: s.parse()?,
            quality,
        })
    }
}

/// Parses a trailing `; q=<qvalue>` weight, scanning backwards from the end of the string.
struct WeightParser<'a>(std::slice::Iter<'a, u8>);

impl<'a> WeightParser<'a> {
    /// Returns the length of the string preceding the weight, and the weight.
    fn parse(s: &'a str) -> Option<(usize, u16)> {
        let mut parser = WeightParser(s.as_bytes().iter());
        let qvalue = parser.qvalue()?;
        parser.eat(b'=')?;
        parser.eat(b'q').or_else(|| parser.eat(b'Q'))?;
        parser.ows();
        parser.eat(b';')?;
        parser.ows();
        Some((parser.0.as_slice().len(), qvalue))
    }

    fn qvalue(&mut self) -> Option<u16> {
        let mut qvalue = match self.digit() {
            Some(v @ 0) | Some(v @ 1) if self.peek() == Some(b'=') => return Some(v * 1000),
            Some(v) => v,
            None if self.peek() == Some(b'.') => 0,
            None => return None,
        };

        match self.digit() {
            Some(digit1) => match self.digit() {
                Some(digit2) => qvalue += digit1 * 10 + digit2 * 100,
                None => {
                    qvalue *= 10;
                    qvalue += digit1 * 100;
                }
            },
            None => qvalue *= 100,
        }

        self.eat(b'.')?;

        match self.peek()? {
            b'0' => {
                self.next();
                Some(qvalue)
            }
            b'1' if qvalue == 0 => {
                self.next();
                Some(1000)
            }
            _ => None,
        }
    }

    fn digit(&mut self) -> Option<u16> {
        match self.peek()? {
            v @ b'0'..=b'9' => {
                self.next();
                Some((v - b'0') as u16)
            }
            _ => None,
        }
    }

    fn ows(&mut self) {
        while let Some(b' ') | Some(b'\t') = self.peek() {
            self.next();
        }
    }

    fn peek(&self) -> Option<u8> {
        self.0.clone().next_back().cloned()
    }

    fn next(&mut self) -> Option<u8> {
        self.0.next_back().cloned()
    }

    fn eat(&mut self, value: u8) -> Option<()> {
        if self.peek() == Some(value) {
            self.next();
            Some(())
        } else {
            None
        }
    }
}

/// Reads the `Accept` header. An absent header yields `None`.
pub fn get_accept(headers: &HeaderMap) -> HeaderResult<Option<Vec<AcceptEntry>>> {
    parse_comma_delimited(headers, http::header::ACCEPT)
}

/// A content coding from a `Content-Encoding` header.
#[derive(Debug, PartialEq, Eq)]
pub enum Encoding {
    Identity,
    Gzip,
    Deflate,
    /// Any other valid coding, in lowercase.
    Other(String),
}

impl FromStr for Encoding {
    type Err = ();

    fn from_str(s: &str) -> Result<Encoding, ()> {
        let coding = match s.to_ascii_lowercase().as_str() {
            "identity" => Encoding::Identity,
            "gzip" | "x-gzip" => Encoding::Gzip,
            "deflate" => Encoding::Deflate,
            "x-compress" => Encoding::Other("compress".to_string()),
            _ if is_token(s) => Encoding::Other(s.to_ascii_lowercase()),
            _ => return Err(()),
        };
        Ok(coding)
    }
}

/// Reads the `Content-Encoding` header as a list of codings, in order. An absent header yields an empty list.
pub fn get_content_encoding(headers: &HeaderMap) -> HeaderResult<Vec<Encoding>> {
    match parse_comma_delimited(headers, http::header::CONTENT_ENCODING)? {
        Some(codings) if codings.is_empty() => Err(TOO_FEW_VALUES.to_string()),
        Some(codings) => Ok(codings),
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
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(value[6..].trim())
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

#[cfg(test)]
mod test {
    use super::*;
    use http::header::{HeaderName, ACCEPT, CONTENT_ENCODING, CONTENT_TYPE};

    fn map(name: HeaderName, values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(name.clone(), HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    fn accept(values: &[&str]) -> Vec<(String, u16)> {
        get_accept(&map(ACCEPT, values))
            .unwrap()
            .unwrap()
            .into_iter()
            .map(|e| (e.mime.to_string(), e.quality))
            .collect()
    }

    fn encoding(values: &[&str]) -> HeaderResult<Vec<Encoding>> {
        get_content_encoding(&map(CONTENT_ENCODING, values))
    }

    #[test]
    fn accept_parses_qualities() {
        assert_eq!(
            accept(&["audio/*; q=0.2, audio/basic"]),
            vec![
                ("audio/*".to_string(), 200),
                ("audio/basic".to_string(), 1000)
            ]
        );
        assert_eq!(
            accept(&["text/plain; q=0.5, text/html, text/x-dvi; q=0.8, text/x-c"]),
            vec![
                ("text/plain".to_string(), 500),
                ("text/html".to_string(), 1000),
                ("text/x-dvi".to_string(), 800),
                ("text/x-c".to_string(), 1000),
            ]
        );
        assert_eq!(accept(&["*/*;q=0"]), vec![("*/*".to_string(), 0)]);
        assert_eq!(accept(&["*/*;Q=1.0"]), vec![("*/*".to_string(), 1000)]);
    }

    #[test]
    fn accept_skips_empty_elements() {
        assert_eq!(
            accept(&["application/json,"]),
            vec![("application/json".to_string(), 1000)]
        );
        assert_eq!(accept(&[""]), vec![]);
    }

    #[test]
    fn accept_treats_malformed_quality_as_a_mime_parameter() {
        assert_eq!(accept(&["*/*;q=.5"]), vec![("*/*;q=.5".to_string(), 1000)]);
        assert_eq!(
            accept(&["application/json; q=abc"]),
            vec![("application/json; q=abc".to_string(), 1000)]
        );
    }

    #[test]
    fn accept_merges_multiple_header_lines() {
        assert_eq!(
            accept(&["text/html", "application/json"]),
            vec![
                ("text/html".to_string(), 1000),
                ("application/json".to_string(), 1000)
            ]
        );
    }

    #[test]
    fn accept_absent_or_invalid() {
        assert!(get_accept(&HeaderMap::new()).unwrap().is_none());
        assert!(get_accept(&map(ACCEPT, &["not a mime"])).is_err());
    }

    #[test]
    fn content_encoding_parses_codings() {
        assert_eq!(encoding(&[]).unwrap(), vec![]);
        assert_eq!(encoding(&["gzip"]).unwrap(), vec![Encoding::Gzip]);
        assert_eq!(encoding(&["GZIP"]).unwrap(), vec![Encoding::Gzip]);
        assert_eq!(encoding(&["x-gzip"]).unwrap(), vec![Encoding::Gzip]);
        assert_eq!(encoding(&["deflate"]).unwrap(), vec![Encoding::Deflate]);
        assert_eq!(encoding(&["identity"]).unwrap(), vec![Encoding::Identity]);
        assert_eq!(
            encoding(&["x-compress"]).unwrap(),
            vec![Encoding::Other("compress".to_string())]
        );
        assert_eq!(
            encoding(&["Br"]).unwrap(),
            vec![Encoding::Other("br".to_string())]
        );
    }

    #[test]
    fn content_encoding_skips_empty_elements_and_merges_lines() {
        assert_eq!(encoding(&["gzip,"]).unwrap(), vec![Encoding::Gzip]);
        assert_eq!(
            encoding(&["identity", "gzip"]).unwrap(),
            vec![Encoding::Identity, Encoding::Gzip]
        );
    }

    #[test]
    fn content_encoding_rejects_empty_and_invalid_values() {
        assert!(encoding(&[""]).is_err());
        assert!(encoding(&[" , "]).is_err());
        assert!(encoding(&["gz ip"]).is_err());
    }

    #[test]
    fn content_type_parses_single_value() {
        let content_type = |values: &[&str]| get_content_type(&map(CONTENT_TYPE, values));
        assert_eq!(content_type(&[]).unwrap(), None);
        assert_eq!(
            content_type(&["Application/JSON"]).unwrap(),
            Some(mime::APPLICATION_JSON)
        );
        assert_eq!(
            content_type(&["application/json; charset=utf-8"])
                .unwrap()
                .unwrap()
                .to_string(),
            "application/json; charset=utf-8"
        );
        assert!(content_type(&["nonsense"]).is_err());
    }

    #[test]
    fn content_type_rejects_multiple_values() {
        assert_eq!(
            get_content_type(&map(CONTENT_TYPE, &["application/json", "text/plain"])),
            Err("too many header values".to_string())
        );
    }
}
