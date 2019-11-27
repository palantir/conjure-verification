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

use core;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::string::ToString;

use bytes::Bytes;
use either::{Either, Left, Right};
use http::Method;
use serde_json;

use conjure::value::*;
use conjure_verification_common::conjure::value::de_plain::deserialize_plain;
use conjure_verification_error::Result;
use conjure_verification_error::{Code, Error};
use conjure_verification_http::error::ConjureVerificationError;
use conjure_verification_http::request::Format;
use conjure_verification_http::request::Request;
use conjure_verification_http::resource::Resource;
use conjure_verification_http::resource::Route;
use conjure_verification_http::response::IntoResponse;
use conjure_verification_http::response::NoContent;
use conjure_verification_http::response::Response;
use conjure_verification_http::SerializableFormat;
use conjure_verification_http_server::RouteWithOptions;
use errors::*;
use fixed_streaming::StreamingResponse;
use raw_json::RawJson;
use resolved_test_cases::ResolvedClientTestCases;
use resolved_test_cases::ResolvedPositiveAndNegativeTestCases;
use resolved_test_cases::ResolvedTestCase;
use resolved_test_cases::ResolvedTestCases;
use test_spec::EndpointName;
use typed_headers::{ContentLength, ContentType, HeaderMapExt};
use DynamicResource;

pub struct SpecTestResource {
    test_cases: Box<ResolvedClientTestCases>,
}

impl SpecTestResource {
    pub fn new(test_cases: Box<ResolvedClientTestCases>) -> SpecTestResource {
        SpecTestResource { test_cases }
    }

    /// Create a test that validates that some param from the request is as expected.
    /// The comparison is done by deserializing both sides to [ConjureValue], the test case json
    /// using deser_json, and the param value using deser_plain.
    fn create_param_test<F, G>(
        endpoint: EndpointName,
        get_param: F,
        get_cases: G,
    ) -> impl Fn(&SpecTestResource, &mut Request) -> Result<NoContent> + Sync
    where
        // TODO return Result<Option<&str>>
        F: Fn(&mut Request) -> Result<Option<String>> + Sync + Send,
        G: Fn(&ResolvedClientTestCases) -> &HashMap<EndpointName, ResolvedTestCases> + Sync + Send,
    {
        move |resource: &SpecTestResource, request: &mut Request| -> Result<_> {
            let index = SpecTestResource::parse_index(request)?;
            let param_str = get_param(request)?;
            let validate =
                |request: &mut Request| SpecTestResource::assert_no_request_body(request);

            validate(request)?;

            let ResolvedTestCases {
                test_cases,
                conjure_type,
            } = get_endpoint(get_cases(&resource.test_cases), &endpoint)?;

            let resolved_test_case: &ResolvedTestCase = {
                test_cases.get(index).ok_or_else(|| {
                    Error::new_safe(
                        "Index out of bounds",
                        VerificationError::IndexOutOfBounds {
                            index,
                            max_index: test_cases.len(),
                        },
                    )
                })
            }?;
            let expected_param_str: &str = resolved_test_case.text.as_str();
            let expected_param = &resolved_test_case.value;
            let param = param_str
                .as_ref()
                .map(|str| {
                    let handle_err = |e: Box<StdError + Sync + Send>| {
                        let error_message = format!("{}", e);
                        Error::new_safe(
                            e,
                            VerificationError::param_validation_failure(
                                expected_param_str,
                                expected_param,
                                Some(str.clone()),
                                None,
                                error_message,
                            ),
                        )
                    };

                    deserialize_plain(conjure_type, str.as_str()).map_err(|e| handle_err(e.into()))
                }).unwrap_or_else(|| Ok(ConjureValue::Optional(None)))?;
            if param != *expected_param {
                let error = "Param didn't match expected value";
                return Err(Error::new_safe(
                    error,
                    VerificationError::param_validation_failure(
                        expected_param_str,
                        expected_param,
                        param_str,
                        Some(&param),
                        error,
                    ),
                ));
            }
            Ok(NoContent)
        }
    }

    /// Assert the request body was empty.
    fn assert_no_request_body(request: &mut Request) -> Result<()> {
        let mut request_body = String::new();
        request
            .raw_body()
            .read_to_string(&mut request_body)
            .map_err(|e| Error::new(e, Code::CustomClient))?;
        request_body.is_empty().or_err(|| {
            Error::new_safe(
                "Did not expect a request body",
                VerificationError::UnexpectedBody {
                    body_size: request_body.len(),
                },
            )
        })?;
        Ok(())
    }

    fn response_non_streaming(reply: &str, request: &Request) -> Result<Response> {
        if reply == Bytes::from("null") {
            return NoContent.into_response(request);
        } else {
            return RawJson { data: reply.into() }.into_response(request);
        };
    }

    /// Create an automated test
    fn create_test(
        endpoint: EndpointName,
    ) -> impl Fn(&SpecTestResource, &mut Request) -> Result<Response> + Sync + Send {
        // Expects an index
        move |resource: &SpecTestResource, request: &mut Request| -> Result<Response> {
            let index: TestIndex = SpecTestResource::parse_index(request)?.into();

            // Perform all assertions in this block, because if they fail, we want to catch the
            // error and record it.
            let validate =
                |request: &mut Request| SpecTestResource::assert_no_request_body(request);

            validate(request)?;

            let cases = get_endpoint(&resource.test_cases.auto_deserialize, &endpoint)?;
            return get_test_case_at_index(cases, &index)?
                .map_left(|case| match &case.0.value {
                    ConjureValue::Primitive(ConjurePrimitiveValue::Binary(binary)) => {
                        StreamingResponse {
                            data: binary.0.to_owned(),
                        }.into_response(request)
                    }
                    _ => SpecTestResource::response_non_streaming(case.0.text.as_str(), request),
                }).map_right(|case| SpecTestResource::response_non_streaming(case.0, request))
                .into_inner();
        }
    }

    fn parse_index(request: &Request) -> Result<usize> {
        request
            .path_param("index")
            .parse()
            .map_err(|err| Error::new_safe(err, Code::InvalidArgument))
    }

    /// Returns a `VerificationError::ConfirmationFailure` if the result is not what was expected.
    fn confirm(&self, request: &mut Request) -> Result<NoContent> {
        let index: usize = SpecTestResource::parse_index(request)?;
        let endpoint = EndpointName::new(request.path_param("endpoint"));

        let positive_cases = &get_endpoint(&self.test_cases.auto_deserialize, &endpoint)?.positive;

        let conjure_type = &positive_cases.conjure_type;
        let resolved_test_case = positive_cases.test_cases.get(index).ok_or_else(|| {
            Error::new_safe(
                "Index out of bounds",
                VerificationError::IndexOutOfBounds {
                    index,
                    max_index: positive_cases.test_cases.len(),
                },
            )
        })?;
        let expected_body_str = resolved_test_case.text.to_string();
        let expected_body: &ConjureValue = &resolved_test_case.value;

        // TODO(dsanduleac): we don't currently handle binary (streaming) requests, we should have
        // a dedicated deserializer for conjure::Value from Request, which knows when to read the
        // raw_body() and when to deserialize it to JSON.

        // Special handling for when body is empty - allow no content type (or otherwise expect JSON).
        let request_body_value: serde_json::Value = if let Some(ContentLength(0)) = request
            .headers()
            .typed_get::<ContentLength>()
            .map_err(Error::internal_safe)?
        {
            let mime_opt = request
                .headers()
                .typed_get::<ContentType>()
                .map(|o| o.map(|ct| ct.0))
                .map_err(|e| Error::new_safe(e, Code::InvalidArgument))?;
            if mime_opt.map(|mime| SerializableFormat::Json.matches(&mime)) == Some(false) {
                return Err(Error::new_safe(
                    "unsupported content type",
                    ConjureVerificationError::UnsupportedContentType,
                ));
            };
            serde_json::Value::Null
        } else {
            request.body()?
        };
        let request_body = conjure_type.deserialize(&request_body_value).map_err(|e| {
            let error_message = format!("{}", e);
            Error::new_safe(
                e,
                VerificationError::confirmation_failure(
                    &expected_body_str,
                    expected_body,
                    &request_body_value,
                    None,
                    error_message,
                ),
            )
        })?;
        // Compare request_body with what the test case says we sent
        if request_body != *expected_body {
            let error = "Body didn't match expected Conjure value";
            return Err(Error::new_safe(
                error,
                VerificationError::confirmation_failure(
                    &expected_body_str,
                    expected_body,
                    &request_body_value,
                    Some(&request_body),
                    error,
                ),
            ));
        }
        Ok(NoContent)
    }
}

impl Resource for SpecTestResource {
    const BASE_PATH: &'static str = "";

    fn register<R>(_: &mut R)
    where
        R: Route<Self>,
    {
    }
}

/// The full index among `PositiveAndNegativeTests` where positives start at index 0, and after them
/// come the negative tests.
#[derive(Debug, Eq, Ord, PartialOrd, PartialEq, From, Hash, Display)]
pub struct TestIndex(usize);

#[derive(Debug, From)]
pub struct AutoDeserializePositiveTest<'a>(pub &'a ResolvedTestCase);

#[derive(Debug, From)]
pub struct AutoDeserializeNegativeTest<'a>(pub &'a str);

fn get_test_case_at_index<'a>(
    cases: &'a ResolvedPositiveAndNegativeTestCases,
    index: &TestIndex,
) -> Result<Either<AutoDeserializePositiveTest<'a>, AutoDeserializeNegativeTest<'a>>> {
    let positives = cases.positive.test_cases.len();
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
        Right(test.as_str().into())
    } else {
        Left(cases.positive.test_cases[index.0].borrow().into())
    };
    Ok(result)
}

impl DynamicResource for SpecTestResource {
    fn register<R>(&self, router: &mut R)
    where
        R: Route<Self>,
    {
        // Endpoint to send the the received data to.
        router.route_with_options(
            Method::POST,
            "/confirm/:endpoint/:index",
            SpecTestResource::confirm,
        );

        // Wire up all automatic endpoint names.
        let automatic_endpoint_names = self.test_cases.auto_deserialize.keys();

        for endpoint_name in automatic_endpoint_names.cloned() {
            router.route_with_options(
                Method::GET,
                format!("/body/{}/:index", endpoint_name.0).as_str(),
                SpecTestResource::create_test(endpoint_name),
            );
        }

        for endpoint_name in self.test_cases.single_path_param_service.keys().cloned() {
            router.route_with_options(
                Method::POST,
                format!("/single-path-param/{}/:index/:param", endpoint_name.0).as_str(),
                SpecTestResource::create_param_test(
                    endpoint_name,
                    |req| Ok(Some(req.path_param("param").into())),
                    |tests| &tests.single_path_param_service,
                ),
            );
        }

        for endpoint_name in self.test_cases.single_query_param_service.keys().cloned() {
            router.route_with_options(
                Method::POST,
                format!("/single-query-param/{}/:index", endpoint_name.0).as_str(),
                SpecTestResource::create_param_test(
                    endpoint_name,
                    |req| req.opt_query_param::<String>("foo"),
                    |tests| &tests.single_query_param_service,
                ),
            );
        }

        for endpoint_name in self.test_cases.single_header_service.keys().cloned() {
            router.route_with_options(
                Method::POST,
                format!("/single-header-param/{}/:index", endpoint_name.0).as_str(),
                SpecTestResource::create_param_test(
                    endpoint_name,
                    |req| match req.headers().get::<String>("Some-Header".into()).map(|hv| {
                        hv.to_str()
                            .map(|s| s.to_string())
                            .map_err(|e| Error::new_safe(e, Code::InvalidArgument))
                    }) {
                        Some(result) => result.map(Some),
                        None => Ok(None),
                    },
                    |tests| &tests.single_header_service,
                ),
            );
        }
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

#[cfg(test)]
mod test {
    use std::collections::HashMap;
    use std::sync::Arc;

    use hyper::header::HeaderValue;
    use hyper::HeaderMap;
    use hyper::Method;
    use mime::APPLICATION_JSON;
    use typed_headers::{ContentType, HeaderMapExt};

    use conjure::ir;
    use conjure::resolved_type::builders::*;
    use conjure::resolved_type::OptionalType;
    use conjure::resolved_type::ResolvedType;
    use register_resource;
    use resolved_test_cases;
    use router;
    use router::RouteResult;
    use router::Router;
    use test_spec::ClientTestCases;
    use test_spec::{EndpointName, PositiveAndNegativeTestCases};

    use super::*;
    use conjure_verification_common::type_mapping::builder::*;
    use conjure_verification_common::type_mapping::TestType;

    /// This exists because `Request` takes references only so it can't be used as a builder.
    #[derive(Clone, Default)]
    struct RequestBuilder {
        body: Vec<u8>,
        path_params: HashMap<String, String>,
        query_params: HashMap<String, Vec<String>>,
        headers: HeaderMap<HeaderValue>,
    }

    impl RequestBuilder {
        fn with_request<T>(self, f: impl FnOnce(&mut Request) -> T) -> T {
            let mut body: &[u8] = &self.body[..];
            let mut request: Request = Request::new(
                &self.path_params,
                &self.query_params,
                &self.headers,
                &mut body,
            );
            f(&mut request)
        }
    }

    #[test]
    fn test_header() {
        let router = setup_routes(|cases, types| {
            cases.single_header_service = hashmap!(
                EndpointName::new("string") => vec!["\"yo\"".into()],
                EndpointName::new("int") => vec!["-1234".into()],
                EndpointName::new("bool") => vec!["false".into()],
                EndpointName::new("opt") => vec!["null".into()]
            );
            types.add(
                TestType::SingleHeaderParam,
                EndpointName::new("string"),
                ResolvedType::Primitive(ir::PrimitiveType::String),
            );
            types.add(
                TestType::SingleHeaderParam,
                EndpointName::new("int"),
                ResolvedType::Primitive(ir::PrimitiveType::Integer),
            );
            types.add(
                TestType::SingleHeaderParam,
                EndpointName::new("bool"),
                ResolvedType::Primitive(ir::PrimitiveType::Boolean),
            );
            types.add(
                TestType::SingleHeaderParam,
                EndpointName::new("opt"),
                ResolvedType::Optional(OptionalType {
                    item_type: ResolvedType::Primitive(ir::PrimitiveType::Any).into(),
                }),
            );
        });
        let header_name: &'static str = "Some-Header";
        send_request(
            &router,
            Method::POST,
            "/single-header-param/string/0",
            0,
            |req| {
                req.headers.insert(header_name, "yo".parse().unwrap());
            },
        ).unwrap();
        send_request(
            &router,
            Method::POST,
            "/single-header-param/int/0",
            0,
            |req| {
                req.headers.insert(header_name, "-1234".parse().unwrap());
            },
        ).unwrap();
        send_request(
            &router,
            Method::POST,
            "/single-header-param/bool/0",
            0,
            |req| {
                req.headers.insert(header_name, "false".parse().unwrap());
            },
        ).unwrap();
        send_request(
            &router,
            Method::POST,
            "/single-header-param/opt/0",
            0,
            |_| {},
        ).unwrap();
    }

    #[test]
    fn test_query() {
        let router = setup_routes(|cases, types| {
            cases.single_query_param_service = hashmap!(
                EndpointName::new("string") => vec!["\"yo\"".into()],
                EndpointName::new("int") => vec!["-1234".into()],
                EndpointName::new("bool") => vec!["false".into()],
                EndpointName::new("opt") => vec!["null".into()]
            );
            types.add(
                TestType::SingleQueryParam,
                EndpointName::new("string"),
                ResolvedType::Primitive(ir::PrimitiveType::String),
            );
            types.add(
                TestType::SingleQueryParam,
                EndpointName::new("int"),
                ResolvedType::Primitive(ir::PrimitiveType::Integer),
            );
            types.add(
                TestType::SingleQueryParam,
                EndpointName::new("bool"),
                ResolvedType::Primitive(ir::PrimitiveType::Boolean),
            );
            types.add(
                TestType::SingleQueryParam,
                EndpointName::new("opt"),
                ResolvedType::Optional(OptionalType {
                    item_type: ResolvedType::Primitive(ir::PrimitiveType::Any).into(),
                }),
            );
        });
        send_request(
            &router,
            Method::POST,
            "/single-query-param/string/0",
            0,
            |req| {
                req.query_params.insert("foo".into(), vec!["yo".into()]);
            },
        ).unwrap();
        send_request(
            &router,
            Method::POST,
            "/single-query-param/int/0",
            0,
            |req| {
                req.query_params.insert("foo".into(), vec!["-1234".into()]);
            },
        ).unwrap();
        send_request(
            &router,
            Method::POST,
            "/single-query-param/bool/0",
            0,
            |req| {
                req.query_params.insert("foo".into(), vec!["false".into()]);
            },
        ).unwrap();
        send_request(
            &router,
            Method::POST,
            "/single-query-param/opt/0",
            0,
            |_| {},
        ).unwrap();
    }

    #[test]
    fn test_validation_error() {
        let (_, router, _) = setup_simple_auto_positive();

        match send_request(&router, Method::GET, "/body/foo/0", 0, |req| {
            req.body = "bad body".into();
        }) {
            Err(err) => assert_eq!(err.name(), "ConjureVerification:UnexpectedBody"),
            _ => panic!("Bad request didn't fail validation checks"),
        }
    }

    #[test]
    fn test_confirm() {
        let (expected_body, router, _) = setup_simple_auto_positive();

        // Test that confirmation responds with NOT_ACCEPTABLE for an incorrect body.
        confirm_with(&router, "bad".into(), Some(Code::InvalidArgument));
        // Test that confirmation works with the correct body.
        confirm_with(&router, expected_body.into(), None);
    }

    #[test]
    fn test_confirm_binary() {
        let (expected_body, router, _) = setup_simple_auto_positive_binary();

        // Test that confirmation responds with NOT_ACCEPTABLE for an incorrect body.
        confirm_with(&router, "bad".into(), Some(Code::InvalidArgument));
        // Test that confirmation works with the correct body.
        confirm_with(&router, expected_body.into(), None);
    }

    fn confirm_with(router: &Router, body: Vec<u8>, expected_error: Option<Code>) -> () {
        if let RouteResult::Matched { endpoint, .. } = router.route(&Method::POST, "/confirm/foo/0")
        {
            let mut builder = RequestBuilder::default();
            builder.path_params = hashmap!("index" => "0", "endpoint" => "foo");
            builder.headers.typed_insert(&ContentType(APPLICATION_JSON));
            builder.body = body;
            let result: Result<Response> = builder.with_request(|req| endpoint.handler.handle(req));
            match expected_error {
                Some(code) => assert_eq!(result.err().unwrap().code(), code),
                None => assert!(result.is_ok()),
            }
        } else {
            panic!("Failed to route!")
        }
    }

    fn send_request<F>(
        router: &Router,
        method: Method,
        path: &str,
        index: usize,
        f: F,
    ) -> Result<Response>
    where
        F: FnOnce(&mut RequestBuilder),
    {
        if let RouteResult::Matched { endpoint, .. } = router.route(&method, path) {
            let mut builder = RequestBuilder::default();
            f(&mut builder);
            builder
                .path_params
                .insert("index".into(), index.to_string());
            builder.with_request(|req| endpoint.handler.handle(req))
        } else {
            panic!("Failed to route: {}", path)
        }
    }

    /// Sets up a router handling the desired client test cases.
    fn setup_routes<F>(f: F) -> Router
    where
        F: FnOnce(&mut ClientTestCases, &mut ParamTypesBuilder),
    {
        let mut test_cases = ClientTestCases::default();
        let mut param_types_builder = ParamTypesBuilder::default();
        f(&mut test_cases, &mut param_types_builder);
        let resolved_test_cases =
            resolved_test_cases::resolve_test_cases(&param_types_builder.build(), &test_cases)
                .unwrap();
        let (router, _) = create_resource(resolved_test_cases);
        router
    }

    fn setup_simple_auto_positive_binary() -> (&'static str, Router, Arc<SpecTestResource>) {
        let expected_body = "\"YpbKYYSbQpCjvb754goTMpXaxVX/M2m2287jcpZ3vHI=\"";
        let mut test_cases = ClientTestCases::default();
        test_cases.auto_deserialize = hashmap!(
            EndpointName::new("foo") => PositiveAndNegativeTestCases {
                positive: vec![expected_body.to_string()],
                negative: vec![],
            }
        );
        let mut param_types = ParamTypesBuilder::default();
        param_types.add(
            TestType::Body,
            EndpointName::new("foo"),
            primitive_type(ir::PrimitiveType::Binary),
        );
        let resolved_test_cases =
            resolved_test_cases::resolve_test_cases(&param_types.build(), &test_cases).unwrap();
        let (router, resource) = create_resource(resolved_test_cases);
        (expected_body, router, resource)
    }

    fn setup_simple_auto_positive() -> (&'static str, Router, Arc<SpecTestResource>) {
        let expected_body = "{\"heyo\": 5}";
        let mut test_cases = ClientTestCases::default();
        test_cases.auto_deserialize = hashmap!(
            EndpointName::new("foo") => PositiveAndNegativeTestCases {
                positive: vec![expected_body.to_string()],
                negative: vec![],
            }
        );
        let mut param_types = ParamTypesBuilder::default();
        param_types.add(
            TestType::Body,
            EndpointName::new("foo"),
            object_definition(
                "Name",
                &[field_definition(
                    "heyo",
                    ResolvedType::Primitive(ir::PrimitiveType::Integer),
                )],
            ),
        );
        let resolved_test_cases =
            resolved_test_cases::resolve_test_cases(&param_types.build(), &test_cases).unwrap();
        let (router, resource) = create_resource(resolved_test_cases);
        (expected_body, router, resource)
    }

    fn create_resource(test_cases: ResolvedClientTestCases) -> (Router, Arc<SpecTestResource>) {
        let mut builder = router::Router::builder();
        let resource = Arc::new(SpecTestResource::new(Box::new(test_cases)));
        register_resource(&mut builder, &resource);
        (builder.build(), resource)
    }
}
