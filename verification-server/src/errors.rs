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
        expected_body: String,
        #[error_type(safe)]
        request_body: String,
    },
    #[error_type(code = "InvalidArgument")]
    ParamValidationFailure {
        #[error_type(safe)]
        expected_param: String,
        #[error_type(safe)]
        request_param: String,
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
    pub fn param_validation_failure(
        expected_param: Option<String>,
        request_param: Option<String>,
    ) -> VerificationError {
        VerificationError::ParamValidationFailure {
            expected_param: opt_to_string(expected_param),
            request_param: opt_to_string(request_param),
        }
    }
}

fn opt_to_string(opt: Option<String>) -> String {
    match opt {
        Some(str) => format!("defined: {}", str),
        None => "undefined".into(),
    }
}
