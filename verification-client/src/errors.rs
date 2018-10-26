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

use conjure::value::ConjureValue;
use hyper::StatusCode;
use serde_json;
use std::fmt::Display;
use test_spec::EndpointName;

#[derive(ErrorType)]
#[error_type(namespace = "ConjureVerificationClient")]
pub enum VerificationError {
    #[error_type(code = "InvalidArgument")]
    InvalidEndpointParameter {
        #[error_type(safe)]
        endpoint_name: EndpointName,
    },
    #[error_type(code = "FailedPrecondition")]
    UnexpectedBody { body_size: usize },
    #[error_type(code = "FailedPrecondition")]
    UnexpectedContentType {
        #[error_type(safe)]
        content_type: String,
    },
    #[error_type(code = "FailedPrecondition")]
    ConfirmationFailure {
        #[error_type(safe)]
        expected_body_raw: String,
        #[error_type(safe)]
        expected_body_conjure: String,
        #[error_type(safe)]
        response_body_raw: String,
        #[error_type(safe)]
        response_body_conjure: String,
        #[error_type(safe)]
        cause: String,
    },
    #[error_type(code = "FailedPrecondition")]
    ServerUnderTestConnectionError {
        #[error_type(safe)]
        cause: String,
    },
    #[error_type(code = "InvalidArgument")]
    UnexpectedResponseCode {
        #[error_type(safe)]
        code: StatusCode,
    },
    #[error_type(code = "InvalidArgument")]
    UrlParseFailure { url: String },
    #[error_type(code = "InvalidArgument")]
    IndexOutOfBounds {
        #[error_type(safe)]
        index: usize,
        #[error_type(safe)]
        max_index: usize,
    },
    #[error_type(code = "CustomClient")]
    ClientIo,
}

impl VerificationError {
    pub fn confirmation_failure<E>(
        expected_body_str: &str,
        expected_body: &ConjureValue,
        response_body_str: &serde_json::Value,
        // Option because it might be un-parseable as ConjureValue
        response_body: Option<&ConjureValue>,
        cause: E,
    ) -> VerificationError
    where
        E: Display,
    {
        VerificationError::ConfirmationFailure {
            expected_body_conjure: serde_json::ser::to_string(expected_body).unwrap(),
            expected_body_raw: expected_body_str.to_string(),
            response_body_conjure: response_body
                .map(|rp| serde_json::ser::to_string(rp).unwrap())
                .unwrap_or_else(|| "<undefined>".to_string()),
            response_body_raw: response_body_str.to_string(),
            cause: format!("{}", cause),
        }
    }
}
