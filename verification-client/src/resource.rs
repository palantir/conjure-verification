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
use conjure_verification_http::response::IntoResponse;
use conjure_verification_http::response::NoContent;
use conjure_verification_http_client::body::BytesBody;
use conjure_verification_http_client::Client;
use conjure_verification_http_client::config as client_config;
use conjure_verification_http_client::user_agent::Agent;
use conjure_verification_http_client::user_agent::UserAgent;
use conjure_verification_http_server::RouteWithOptions;
use core;
use either::{Either, Left, Right};
use errors::*;
use http::Method;
use mime::APPLICATION_JSON;
use more_serde_json;
use self::client_config::ServiceConfig;
use self::client_config::ServiceDiscoveryConfig;
use serde_json;
use std::collections::HashMap;
use std::string::ToString;
use test_spec::*;
use zipkin::Endpoint;
use zipkin::Tracer;

pub struct VerificationClientResource {
    test_cases: Box<ServerTestCases>,
    param_types: Box<HashMap<EndpointName, ResolvedType>>,
}

type ParamTypes = HashMap<EndpointName, ResolvedType>;

#[derive(ConjureDeserialize, Debug)]
struct ClientRequest {
    endpoint_name: EndpointName,
    test_case: usize,
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

    fn run_test_case(&self, request: &mut Request) -> Result<impl IntoResponse> {
        let client_request: ClientRequest = request.body()?;

        let endpoint_name = client_request.endpoint_name.clone();
        if let Some(auto_deserialize_cases) = self.test_cases.auto_deserialize.get(&endpoint_name) {
            return self.handle_auto_deserialise_test(&client_request, auto_deserialize_cases);
        }
        Err(Error::new_safe(
            "Unable to find corresponding test case",
            VerificationError::InvalidEndpointParameter { endpoint_name },
        ))
    }

    fn parse_index(request: &Request) -> Result<usize> {
        request
            .path_param("index")
            .parse()
            .map_err(|err| Error::new_safe(err, Code::InvalidArgument))
    }

    fn handle_auto_deserialise_test(
        &self,
        client_request: &ClientRequest,
        auto_deserialize_cases: &PositiveAndNegativeTestCases,
    ) -> Result<impl IntoResponse> {
        let test_case =
            get_test_case_at_index(auto_deserialize_cases, &client_request.test_case.into())?;
        let endpoint = &client_request.endpoint_name;

        let client = VerificationClientResource::construct_client(&client_request.base_url)?;
        let mut builder = client.post("/{endpoint}");
        builder.param("endpoint", &endpoint.0);
        match test_case {
            Left(positive) => {
                let test_body_str = positive.0;
                let conjure_type = get_endpoint(&self.param_types, &endpoint)?;
                let expected_body = deserialize_expected_value(
                    conjure_type,
                    test_body_str.as_str(),
                    &endpoint,
                    client_request.test_case,
                )?;
                let response = builder
                    .body(BytesBody::new(test_body_str.as_str(), APPLICATION_JSON))
                    .send()?;
                if !response.status().is_success() {
                    return Err(Error::new_safe("Wasn't successful", Code::InvalidArgument));
                }

                // We deserialize into serde_json::Value first because .body()'s return type needs
                // to be Deserialize, but the ConjureValue deserializer is a DeserializeSeed
                let response_body_value: serde_json::Value = response.body()?;
                let response_body = conjure_type.deserialize(&response_body_value).map_err(|e| {
                    let error_message = format!("{}", e);
                    Error::new_safe(
                        e,
                        VerificationError::confirmation_failure(
                            &test_body_str,
                            &expected_body,
                            &response_body_value,
                            None,
                            error_message,
                        ),
                    )
                })?;

                // Compare response_body with what the test case says we sent
                if response_body != expected_body {
                    let error = "Body didn't match expected Conjure value";
                    return Err(Error::new_safe(
                        error,
                        VerificationError::confirmation_failure(
                            &test_body_str,
                            &expected_body,
                            &response_body_value,
                            Some(&response_body),
                            "",
                        ),
                    ));
                }
            }

            Right(negative) => {
                let response = builder
                    .body(BytesBody::new(negative.0, APPLICATION_JSON))
                    .send()?;
                if response.status().is_success() {
                    return Err(Error::new_safe(
                        "Unexpected successful response",
                        Code::InvalidArgument,
                    ));
                }
            }
        };
        Ok(NoContent)
    }

    fn construct_client(base_url: &str) -> Result<Client> {
        let service_name = "serviceUnderTest";
        Client::new_static(
            service_name,
            UserAgent::new(Agent::new("conjure-verification-client", "0.0.0")),
            &Tracer::builder().build(Endpoint::builder().build()),
            &ServiceDiscoveryConfig::builder()
                .service(
                    service_name,
                    ServiceConfig::builder()
                        .uris(vec![
                            base_url
                                .parse()
                                // TODO make this better
                                .map_err(|e| Error::new_safe(e, Code::InvalidArgument))?,
                        ]).build(),
                ).build(),
        )
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

fn get_endpoint<'a, V>(
    map: &'a HashMap<EndpointName, V>,
    endpoint: &EndpointName,
) -> Result<&'a V> {
    map.get(endpoint).ok_or_else(|| {
        Error::new_safe(
            "No such endpoint",
            VerificationError::InvalidEndpointParameter {
                endpoint_name: endpoint.clone(),
            },
        )
    })
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
