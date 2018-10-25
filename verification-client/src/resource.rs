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

use conjure::resolved_type::ResolvedType;
use conjure::value::*;
use conjure_verification_error::Code;
use conjure_verification_error::Error;
use conjure_verification_error::Result;
use conjure_verification_http::request::Request;
use conjure_verification_http::resource::Resource;
use conjure_verification_http::resource::Route;
use conjure_verification_http::response::Response;
use conjure_verification_http_client::user_agent::Agent;
use conjure_verification_http_client::user_agent::UserAgent;
use conjure_verification_http_client::Client;
use conjure_verification_http_server::RouteWithOptions;
use core;
use either::{Either, Left, Right};
use errors::*;
use http::Method;
use more_serde_json;
use serde_json;
use std::collections::HashMap;
use std::string::ToString;
use test_spec::*;

pub struct VerificationClientResource {
    test_cases: Box<ServerTestCases>,
    param_types: Box<HashMap<EndpointName, ResolvedType>>,
}

type ParamTypes = HashMap<EndpointName, ResolvedType>;

#[derive(ConjureDeserialize, Debug)]
struct ClientRequest {
    endpoint_name: EndpointName,
    test_case: u64,
    base_url: String,
}

impl VerificationClientResource {
    pub fn new(
        test_cases: Box<ServerTestCases>,
        param_types: Box<ParamTypes>,
    ) -> VerificationClientResource {
        VerificationClientResource {
            test_cases,
            param_types,
        }
    }

    fn run_test_case(&self, request: &mut Request) -> Result<Response> {
        let client_request: ClientRequest = serde_json::from_reader(request.body()?)?;
        if let Some(test_case) = self
            .test_cases
            .auto_deserialize
            .get(clientRequest.endpoint_name)
        {
            return handle_auto_deserialise_test(client_request, test_case);
        }
        Err(Error::new_safe(
            "Unable to find corresponding test case",
            VerificationError::InvalidEndpointParameter {
                endpoint_name: client_request.endpoint_name,
            },
        ))
    }

    fn parse_index(request: &Request) -> Result<usize> {
        request
            .path_param("index")
            .parse()
            .map_err(|err| Error::new_safe(err, Code::InvalidArgument))
    }

    fn handle_auto_deserialise_test(clientRequest: &ClientRequest) -> Result<Response> {
        let client = Client::new_static(
            "serviceUnderTest",
            UserAgent::new(Agent::new("conjure-verification-client", "0.0.0")),
            Tracer::
        );
    }
}

fn deserialize_expected_value(
    conjure_type: &ResolvedType,
    raw: &str,
    endpoint: &EndpointName,
    index: usize,
) -> Result<ConjureValue> {
    more_serde_json::from_str(conjure_type, raw).map_err(|e| {
        Error::internal_safe(e)
            .with_safe_param("endpoint", endpoint.to_string())
            .with_safe_param("index", index)
            .with_safe_param("expected_raw", raw)
    })
}

/// The full index among `PositiveAndNegativeTests` where positives start at index 0, and after them
/// come the negative tests.
#[derive(Debug, Eq, Ord, PartialOrd, PartialEq, From, Hash, Display)]
pub struct TestIndex(usize);

fn get_test_case_at_index(
    cases: &PositiveAndNegativeTestCases,
    index: &TestIndex,
) -> Result<Either<AutoDeserializePositiveTest, AutoDeserializeNegativeTest>> {
    let positives = cases.positive.len();
    let negatives = cases.negative.len();
    let index_out_of_bounds = || {
        Error::new_safe(
            "Index out of bounds",
            VerificationError::IndexOutOfBounds {
                index: index.0,
                max_index: positives + negatives,
            },
        )
    };
    let is_negative_test = index.0 >= positives;
    let result = if is_negative_test {
        let test = cases
            .negative
            .get(index.0 - positives)
            .ok_or_else(index_out_of_bounds)?;
        Right(test.clone().into())
    } else {
        Left(cases.positive[index.0].clone().into())
    };
    Ok(result)
}

impl Resource for VerificationClientResource {
    const BASE_PATH: &'static str = "";

    fn register<R>(router: &mut R)
    where
        R: Route<Self>,
    {
        router.route_with_options(
            Method::POST,
            "/runTestCase",
            VerificationClientResource::run_test_case,
        )
    }
}

trait OrErr<R, E> {
    fn or_err<F>(&self, f: F) -> core::result::Result<R, E>
    where
        F: FnOnce() -> E;
}

impl OrErr<(), Error> for bool {
    fn or_err<F>(&self, f: F) -> core::result::Result<(), Error>
    where
        F: FnOnce() -> Error,
    {
        if *self {
            Ok(())
        } else {
            Err(f())
        }
    }
}
