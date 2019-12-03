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

use std::collections::HashMap;
use std::string::ToString;

use either::{Either, Left, Right};
use hyper::header::ACCEPT;
use hyper::header::HeaderValue;
use hyper::Method;
use hyper::StatusCode;
use mime::APPLICATION_JSON;
use mime::APPLICATION_OCTET_STREAM;
use serde_json;
use typed_headers::{ContentType, HeaderMapExt};
use zipkin::Endpoint;
use zipkin::Tracer;

use conjure::resolved_type::ResolvedType;
use conjure::value::*;
use conjure_verification_common::type_mapping::ParamTypes;
use conjure_verification_common::type_mapping::TestType;
use conjure_verification_error::Error;
use conjure_verification_error::Result;
use conjure_verification_http::request::Request;
use conjure_verification_http::resource::Resource;
use conjure_verification_http::resource::Route;
use conjure_verification_http::response::IntoResponse;
use conjure_verification_http::response::NoContent;
use conjure_verification_http_client::{config as client_config, ResponseBody};
use conjure_verification_http_client::body::BytesBody;
use conjure_verification_http_client::Client;
use conjure_verification_http_client::request::RequestBuilder;
use conjure_verification_http_client::user_agent::Agent;
use conjure_verification_http_client::user_agent::UserAgent;
use conjure_verification_http_server::RouteWithOptions;
use errors::*;
use more_serde_json;
use test_spec::*;

use self::client_config::ServiceConfig;
use self::client_config::ServiceDiscoveryConfig;

lazy_static! {
    static ref USER_AGENT: UserAgent =
        UserAgent::new(Agent::new("conjure-verification-client", "0.0.0"));
}

pub struct VerificationClientResource {
    test_cases: Box<ServerTestCases>,
    param_types: Box<ParamTypes>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug)]
pub(crate) struct ClientRequest {
    pub endpoint_name: EndpointName,
    pub test_case: usize,
    pub base_url: String,
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

        info!(
            "Got request: {}",
            serde_json::to_string(&client_request).map_err(|e| Error::internal(e))?
        );

        let endpoint_name = client_request.endpoint_name.clone();
        if let Some(auto_deserialize_cases) = self.test_cases.auto_deserialize.get(&endpoint_name) {
            return self.handle_auto_deserialize_test(&client_request, auto_deserialize_cases);
        }
        Err(Error::new_safe(
            "Unable to find corresponding test case",
            VerificationError::InvalidEndpointParameter { endpoint_name },
        ))
    }

    fn handle_auto_deserialize_test(
        &self,
        client_request: &ClientRequest,
        auto_deserialize_cases: &PositiveAndNegativeTestCases,
    ) -> Result<impl IntoResponse> {
        let test_case =
            get_test_case_at_index(auto_deserialize_cases, &client_request.test_case.into())?;
        let endpoint = &client_request.endpoint_name;

        let client = VerificationClientResource::construct_client(&client_request.base_url)?;
        let mut builder = client.post("/body/:endpoint");
        builder.param("endpoint", &endpoint.0);
        builder.headers_mut().insert(
            ACCEPT,
            HeaderValue::from_static("*/*; q=0.5, application/json"),
        );
        match test_case {
            Left(positive) => {
                self.check_positive_test_case(client_request, &endpoint, &mut builder, positive)?;
            }

            Right(negative) => {
                VerificationClientResource::check_negative_test_case(&mut builder, negative)?;
            }
        };
        Ok(NoContent)
    }

    fn check_positive_test_case(
        &self,
        client_request: &ClientRequest,
        endpoint: &EndpointName,
        builder: &mut RequestBuilder,
        positive: AutoDeserializePositiveTest,
    ) -> Result<()> {
        let test_body_str = positive.0;
        let response = builder
            .body(BytesBody::new(test_body_str.as_str(), APPLICATION_JSON))
            .send()
            .map_err(|e| {
                // Unpack error cause to expose it to user.
                let cause = e.cause().to_string();
                // TODO format error cause nicely
                Error::new_safe(
                    "Failed to connect to server under test",
                    VerificationError::ServerUnderTestConnectionError { cause },
                )
            })?;

        let response_status = response.status();
        if !response_status.is_success() {
            return Err(Error::new_safe(
                "Wasn't successful",
                VerificationError::UnexpectedResponseCode {
                    code: response_status,
                },
            ));
        }

        // Have to save this before the response is consumed by `Response::body`
        let content_type = response
            .headers()
            .typed_get::<ContentType>()
            .map_err(Error::internal_safe)?;

        let conjure_type = get_endpoint(&self.param_types[&TestType::Body], &endpoint)?;
        let expected_body = deserialize_expected_value(
            conjure_type,
            test_body_str.as_str(),
            &endpoint,
            client_request.test_case,
        )?;

        // Edge case: if we expect an empty value (optional, list, set, map), then the server is
        // also allowed to reply with 204
        if VerificationClientResource::is_empty_container(&expected_body)
            && response_status == StatusCode::NO_CONTENT
        {
            debug!("Accepting 204 response to empty test case {{testCase: {}, endpoint: {}, testCaseContents: {}}}",
                   client_request.test_case, client_request.endpoint_name, test_body_str);
            return Ok(());
        }

        // At this point, we have concluded we don't expect a 204.
        // Thus, we expect a 200 with either OCTET_STREAM or APPLICATION_JSON.
        // Note: we MUST check this before calling .body(), which will fail if there's no content type.
        VerificationClientResource::assert_content_type(
            &content_type,
            &mut vec![APPLICATION_JSON, APPLICATION_OCTET_STREAM]
                .into_iter()
                .map(|mime| Some(ContentType(mime))),
        )?;

        // We deserialize into serde_json::Value first because .body()'s return type needs
        // to be Deserialize, but the ConjureValue deserializer is a DeserializeSeed
        let response_body;
        let response_body_value: serde_json::Value;
        if content_type.unwrap() == ContentType(APPLICATION_JSON) {
            response_body_value = response.body()?;
            response_body = VerificationClientResource::try_parse_response_body(
                conjure_type,
                &response_body_value,
            )?;
        } else {
            let mut raw_body = response.raw_body()?;
            let mut result: Vec<u8> = Vec::new();
            let read_size = raw_body.0.read_to_end(result.as_mut());
            response_body_value = serde_json::Value::String(
                serde_json::to_string(result.as_slice()).map_err(Error::internal)?
            );
            response_body =
                ConjureValue::Primitive(ConjurePrimitiveValue::Binary(Binary(result.to_vec())))
        }

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

        Ok(())
    }

    fn try_parse_response_body(
        conjure_type: &ResolvedType,
        response_body_value: &serde_json::Value,
    ) -> Result<ConjureValue> {
        conjure_type.deserialize(response_body_value).map_err(|e| {
            let error_message = format!("{}", e);
            Error::new_safe(
                e,
                VerificationError::CouldNotParseServerResponse {
                    response_body: response_body_value.to_string(),
                    cause: error_message,
                },
            )
        })
    }

    /// Checks whether the given [ConjureValue] is an "empty container", i.e. it can be deserialized
    /// from a NO_CONTENT response.
    fn is_empty_container(value: &ConjureValue) -> bool {
        match value {
            ConjureValue::Optional(None) => true,
            ConjureValue::List(ref vec) if vec.is_empty() => true,
            ConjureValue::Set(ref vec) if vec.is_empty() => true,
            ConjureValue::Map(ref map) if map.is_empty() => true,
            _ => false,
        }
    }

    fn check_negative_test_case(
        builder: &mut RequestBuilder,
        negative: AutoDeserializeNegativeTest,
    ) -> Result<()> {
        let response = builder
            .body(BytesBody::new(negative.0, APPLICATION_JSON))
            .send()?;
        if !response.status().is_client_error() {
            return Err(Error::new_safe(
                "Unexpected response, expected client error",
                VerificationError::UnexpectedResponseCode {
                    code: response.status(),
                },
            ));
        }
        Ok(())
    }

    /// Assert content-type header matches one of the expected ones.
    fn assert_content_type<ExpectedTypes: Iterator<Item = Option<ContentType>>>(
        response_content_type: &Option<ContentType>,
        expected_content_types: &mut ExpectedTypes,
    ) -> Result<()> {
        if expected_content_types.any(|expected| expected == *response_content_type) {
            Ok(())
        } else {
            return Err(Error::new_safe(
                "Did not expect content type",
                VerificationError::UnexpectedContentType {
                    content_type: response_content_type
                        .as_ref()
                        .map(|ct| ct.to_string())
                        .unwrap_or("<empty>".to_string()),
                },
            ));
        }
    }

    fn construct_client(base_url: &str) -> Result<Client> {
        let service_name = "serviceUnderTest";
        Client::new_static(
            service_name,
            USER_AGENT.clone(),
            &Tracer::builder().build(Endpoint::builder().build()),
            &ServiceDiscoveryConfig::builder()
                .service(
                    service_name,
                    // don't retry as that gives better error message if client fails
                    ServiceConfig::builder()
                        .max_num_retries(0)
                        .uris(vec![base_url.parse().map_err(|e| {
                            Error::new_safe(
                                e,
                                VerificationError::UrlParseFailure {
                                    url: base_url.to_string(),
                                },
                            )
                        })?]).build(),
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
