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
use bytes::Bytes;
use conjure_verification_error::Result;
use conjure_verification_error::{Code, Error};
use conjure_verification_http::request::Request;
use conjure_verification_http::resource::Resource;
use conjure_verification_http::resource::Route;
use conjure_verification_http::response::IntoResponse;
use conjure_verification_http::response::NoContent;
use conjure_verification_http::response::Response;
use core;
use errors::*;
use http::status::StatusCode;
use http::Method;
use hyper::header::HeaderValue;
use raw_json::RawJson;
use serde_json;
use serde_json::Value;
use std::collections::HashMap;
use std::string::ToString;
use test_spec::ClientTestCases;
use test_spec::EndpointName;
use test_spec::TestIndex;
use DynamicResource;

pub struct SpecTestResource {
    test_cases: Box<ClientTestCases>,
}

impl SpecTestResource {
    pub fn new(test_cases: ClientTestCases) -> SpecTestResource {
        SpecTestResource {
            test_cases: Box::new(test_cases),
        }
    }

    fn decode_json_as_param_string(json: &str) -> Result<Option<String>> {
        let value: Value = serde_json::from_str(json).map_err(Error::internal_safe)?;
        let result = match value {
            Value::Bool(bool) => Some(bool.to_string()),
            Value::Number(number) => Some(number.to_string()),
            Value::String(string) => Some(string),
            Value::Null => None,
            v => {
                Err(Error::internal_safe("Not supported").with_safe_param("value", v.to_string()))?
            }
        };
        Ok(result)
    }

    /// Create a test that validates that some param from the request is as expected.
    /// The comparison is done on strings.
    /// Converting the test case JSON to the actual expected value is done by
    /// [decode_json_as_param_string].
    fn create_param_test<F, G>(
        endpoint: EndpointName,
        get_param: F,
        get_cases: G,
    ) -> impl Fn(&SpecTestResource, &mut Request) -> Result<NoContent> + Sync
    where
        F: Fn(&mut Request) -> Result<Option<String>> + Sync + Send,
        G: Fn(&ClientTestCases) -> &HashMap<EndpointName, Vec<String>> + Sync + Send,
    {
        move |resource: &SpecTestResource, request: &mut Request| -> Result<_> {
            let index = SpecTestResource::parse_index(request)?;
            let param = get_param(request)?;
            let validate =
                |request: &mut Request| SpecTestResource::assert_no_request_body(request);

            validate(request)?;
            let test_cases = test_cases_for_endpoint(get_cases(&resource.test_cases), &endpoint)?;
            let expected_param = {
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
            let expected_param = SpecTestResource::decode_json_as_param_string(expected_param)?;
            if param != expected_param {
                return Err(Error::new_safe(
                    "Param didn't match expected value",
                    VerificationError::param_validation_failure(expected_param, param),
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

            let cases = test_cases_for_endpoint(&resource.test_cases.auto_deserialize, &endpoint)?;
            let reply: Bytes = cases
                .index(&index)?
                .map_left(|case| case.0)
                .map_right(|case| case.0)
                .into_inner()
                .into();

            RawJson { data: reply }.into_response(request)
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

        let expected_body = {
            let positive_cases =
                &test_cases_for_endpoint(&self.test_cases.auto_deserialize, &endpoint)?.positive;
            positive_cases.get(index).ok_or_else(|| {
                Error::new_safe(
                    "Index out of bounds",
                    VerificationError::IndexOutOfBounds {
                        index,
                        max_index: positive_cases.len(),
                    },
                )
            })
        }?.to_string();
        let expected_body_value: Value =
            serde_json::from_str(&*expected_body).map_err(Error::internal_safe)?;
        let request_body_value: Value = request.body()?;
        // Compare request_body with what the test case says we sent
        if request_body_value != expected_body_value {
            return Err(Error::new_safe(
                "Body didn't match expected JSON value",
                VerificationError::ConfirmationFailure {
                    expected_body,
                    request_body: serde_json::to_string(&request_body_value)
                        .map_err(|e| Error::new_safe(e, Code::CustomClient))?,
                },
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

/// A trait that I derive automatically for things that have Route<T>, which allows binding a route
/// to the desired method and also to OPTIONS with a default handler for the latter.
trait RouteWithOptions<T>: Route<T> {
    /// Creates a route but adds an OPTIONS endpoint to it as well.
    fn route_with_options<F, R>(&mut self, method: Method, route: &str, f: F)
    where
        F: Fn(&T, &mut Request) -> Result<R> + 'static + Sync + Send,
        R: 'static + IntoResponse,
    {
        assert_ne!(method, Method::OPTIONS);
        self.route(method, route, "", f);
        self.route(Method::OPTIONS, route, "", |_, req| Self::options(req));
    }

    /// To support pre-flight requests sent by browsers in CORS mode.
    /// See <https://stackoverflow.com/questions/29954037/why-is-an-options-request-sent-and-can-i-disable-it>
    fn options(_request: &mut Request) -> Result<Response> {
        let mut response = Response::new(StatusCode::OK);
        {
            let headers = &mut response.headers;
            headers.append("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
            headers.append(
                "Access-Control-Allow-Methods",
                HeaderValue::from_static("POST, GET, OPTIONS"),
            );
            headers.append(
                "Access-Control-Allow-Headers",
                // single-header-service.conjure.yml uses 'Some-Header', so we need to whitelist it in preflight checks
                HeaderValue::from_static("Content-Type, Some-Header"),
            );
        }
        Ok(response)
    }
}

impl<T, X> RouteWithOptions<T> for X
where
    X: Route<T>,
{
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
                format!("/{}/:index", endpoint_name.0).as_str(),
                SpecTestResource::create_test(endpoint_name),
            );
        }

        for endpoint_name in self.test_cases.single_path_param_service.keys().cloned() {
            router.route_with_options(
                Method::POST,
                format!("/{}/:index/:param", endpoint_name.0).as_str(),
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
                format!("/{}/:index", endpoint_name.0).as_str(),
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
                format!("/{}/:index", endpoint_name.0).as_str(),
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

fn test_cases_for_endpoint<'a, V>(
    map: &'a HashMap<EndpointName, V>,
    endpoint: &EndpointName,
) -> Result<&'a V> {
    map.get(endpoint).ok_or_else(|| {
        Error::internal_safe("No such endpoint").with_safe_param("endpointName", endpoint)
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
    use super::*;
    use hyper::HeaderMap;
    use hyper::Method;
    use mime::APPLICATION_JSON;
    use register_resource;
    use router;
    use router::RouteResult;
    use router::Router;
    use std::collections::HashMap;
    use std::sync::Arc;
    use test_spec::*;
    use typed_headers::{ContentType, HeaderMapExt};

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
        let router = setup_routes(|cases| {
            cases.single_header_service = hashmap!(
                EndpointName::new("string") => vec!["\"yo\"".into()],
                EndpointName::new("int") => vec!["-1234".into()],
                EndpointName::new("bool") => vec!["false".into()],
                EndpointName::new("opt") => vec!["null".into()]
            );
        });
        let header_name: &'static str = "Some-Header";
        send_request(&router, Method::POST, "/string/0", 0, |req| {
            req.headers.insert(header_name, "yo".parse().unwrap());
        }).unwrap();
        send_request(&router, Method::POST, "/int/0", 0, |req| {
            req.headers.insert(header_name, "-1234".parse().unwrap());
        }).unwrap();
        send_request(&router, Method::POST, "/bool/0", 0, |req| {
            req.headers.insert(header_name, "false".parse().unwrap());
        }).unwrap();
        send_request(&router, Method::POST, "/opt/0", 0, |_| {}).unwrap();
    }

    #[test]
    fn test_query() {
        let router = setup_routes(|cases| {
            cases.single_query_param_service = hashmap!(
                EndpointName::new("string") => vec!["\"yo\"".into()],
                EndpointName::new("int") => vec!["-1234".into()],
                EndpointName::new("bool") => vec!["false".into()],
                EndpointName::new("opt") => vec!["null".into()]
            );
        });
        send_request(&router, Method::POST, "/string/0", 0, |req| {
            req.query_params.insert("foo".into(), vec!["yo".into()]);
        }).unwrap();
        send_request(&router, Method::POST, "/int/0", 0, |req| {
            req.query_params.insert("foo".into(), vec!["-1234".into()]);
        }).unwrap();
        send_request(&router, Method::POST, "/bool/0", 0, |req| {
            req.query_params.insert("foo".into(), vec!["false".into()]);
        }).unwrap();
        send_request(&router, Method::POST, "/opt/0", 0, |_| {}).unwrap();
    }

    #[test]
    fn test_validation_error() {
        let (_, router, _) = setup_simple_auto_positive();

        match send_request(&router, Method::GET, "/foo/0", 0, |req| {
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

    fn confirm_with(router: &Router, body: Vec<u8>, expected_error: Option<Code>) -> () {
        if let RouteResult::Matched { endpoint, .. } = router.route(Method::POST, "/confirm/foo/0")
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
        if let RouteResult::Matched { endpoint, .. } = router.route(method, path) {
            let mut builder = RequestBuilder::default();
            f(&mut builder);
            builder
                .path_params
                .insert("index".into(), index.to_string());
            builder.with_request(|req| endpoint.handler.handle(req))
        } else {
            panic!("Failed to route!")
        }
    }

    /// Sets up a router handling the desired client test cases.
    fn setup_routes(f: impl FnOnce(&mut ClientTestCases)) -> Router {
        let mut test_cases = ClientTestCases::default();
        f(&mut test_cases);
        let (router, _) = create_resource(test_cases);
        router
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
        let (router, resource) = create_resource(test_cases);
        (expected_body, router, resource)
    }

    fn create_resource(test_cases: ClientTestCases) -> (Router, Arc<SpecTestResource>) {
        let mut builder = router::Router::builder();
        let resource = Arc::new(SpecTestResource::new(test_cases));
        register_resource(&mut builder, &resource);
        (builder.build(), resource)
    }
}
