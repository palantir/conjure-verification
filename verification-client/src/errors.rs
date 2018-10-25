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
use serde_json;
use std::fmt::Display;
use test_spec::EndpointName;

#[derive(ErrorType)]
#[error_type(namespace = "ConjureVerification")]
pub enum VerificationError {
    #[error_type(code = "InvalidArgument")]
    InvalidEndpointParameter {
        #[error_type(safe)]
        endpoint_name: EndpointName,
    },
    #[error_type(code = "InvalidArgument")]
    UnexpectedBody { body_size: usize },
    #[error_type(code = "InvalidArgument")]
    UnexpectedContentType {
        #[error_type(safe)]
        content_type: String,
    },
    #[error_type(code = "InvalidArgument")]
    ConfirmationFailure {
        #[error_type(safe)]
        expected_body_raw: String,
        #[error_type(safe)]
        expected_body_conjure: String,
        #[error_type(safe)]
        request_body_raw: String,
        #[error_type(safe)]
        request_body_conjure: String,
        #[error_type(safe)]
        cause: String,
    },
    #[error_type(code = "InvalidArgument")]
    ParamValidationFailure {
        #[error_type(safe)]
        expected_param_conjure: String,
        #[error_type(safe)]
        expected_param_raw: String,
        #[error_type(safe)]
        request_param_conjure: String,
        #[error_type(safe)]
        request_param_raw: String,
        #[error_type(safe)]
        cause: String,
    },
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
    pub fn param_validation_failure<E>(
        expected_param_str: &str,
        expected_param: &ConjureValue,
        // Option because it might be undefined
        request_param_str: Option<String>,
        request_param: Option<&ConjureValue>,
        cause: E,
    ) -> VerificationError
    where
        E: Display,
    {
        VerificationError::ParamValidationFailure {
            expected_param_conjure: serde_json::ser::to_string(expected_param).unwrap(),
            expected_param_raw: expected_param_str.to_string(),
            request_param_conjure: request_param
                .map(|rp| serde_json::ser::to_string(rp).unwrap())
                .unwrap_or_else(|| "<undefined>".to_string()),
            request_param_raw: request_param_str.unwrap_or_else(|| "<undefined>".to_string()),
            cause: format!("{}", cause),
        }
    }

    pub fn confirmation_failure<E>(
        expected_body_str: &str,
        expected_body: &ConjureValue,
        request_body_str: &serde_json::Value,
        // Option because it might be un-parseable as ConjureValue
        request_body: Option<&ConjureValue>,
        cause: E,
    ) -> VerificationError
    where
        E: Display,
    {
        VerificationError::ConfirmationFailure {
            expected_body_conjure: serde_json::ser::to_string(expected_body).unwrap(),
            expected_body_raw: expected_body_str.to_string(),
            request_body_conjure: request_body
                .map(|rp| serde_json::ser::to_string(rp).unwrap())
                .unwrap_or_else(|| "<undefined>".to_string()),
            request_body_raw: request_body_str.to_string(),
            cause: format!("{}", cause),
        }
    }
}
