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

#[allow(unused_imports)]
use conjure_verification_common::conjure;

use self::type_builders::*;
use bytes::Bytes;
use conjure::ir;
use conjure::resolved_type::ResolvedType;
use conjure_verification_error::Result;
use conjure_verification_http::request::Request;
use conjure_verification_http::resource::Resource;
use conjure_verification_http::resource::Route;
use conjure_verification_http::response::Body;
use conjure_verification_http::response::IntoResponse;
use conjure_verification_http::response::NoContent;
use conjure_verification_http::response::Response;
use hyper::header::HeaderValue;
use hyper::HeaderMap;
use hyper::Method;
use hyper::StatusCode;
use mime::APPLICATION_JSON;
use resource::*;
use router;
use router::RouteResult;
use router::Router;
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use test_spec::ServerTestCases;
use test_spec::{EndpointName, PositiveAndNegativeTestCases};
use url::Url;

#[test]
fn test_content_type_error() {
    let endpoint_name = "returns_non_json_10_body";
    let conjure_type = ResolvedType::Primitive(ir::PrimitiveType::Integer);
    let router = setup::setup_simple_auto_positive(json!(10), endpoint_name, conjure_type);

    run_test_case_against_server(
        &router,
        endpoint_name,
        Some("ConjureVerificationClient:UnexpectedContentType"),
    );
}

/// Test a bad response from the server-under-test that is still parseable with the expected conjure type.
#[test]
fn test_confirmation_error() {
    let endpoint_name = "returns_string_foo";
    let conjure_type = ResolvedType::Primitive(ir::PrimitiveType::String);
    // Sending it a different string!
    let router = setup::setup_simple_auto_positive(json!("bar"), endpoint_name, conjure_type);

    run_test_case_against_server(
        &router,
        endpoint_name,
        Some("ConjureVerificationClient:ConfirmationFailure"),
    );
}

/// Test a bad response from the server-under-test that has a different structure than expected.
#[test]
fn test_response_parse_error() {
    let endpoint_name = "returns_empty_object";
    let conjure_type = ResolvedType::Primitive(ir::PrimitiveType::Integer);
    let router = setup::setup_simple_auto_positive(json!(5), endpoint_name, conjure_type);

    run_test_case_against_server(
        &router,
        endpoint_name,
        Some("ConjureVerificationClient:CouldNotParseServerResponse"),
    );
}

/// Test missing fields in the response are ok if their values were 'empty'.
#[test]
fn test_response_empty_missing_fields_ok() {
    let endpoint_name = "returns_empty_object";
    let conjure_type = object_definition(
        "foo",
        &[
            field_definition(
                "missing_optional",
                optional_type(primitive_type(ir::PrimitiveType::Integer)),
            ),
            field_definition(
                "missing_list",
                list_type(primitive_type(ir::PrimitiveType::Integer)),
            ),
            field_definition(
                "missing_set",
                set_type(primitive_type(ir::PrimitiveType::Integer)),
            ),
            field_definition(
                "missing_map",
                map_type(
                    ir::PrimitiveType::String,
                    primitive_type(ir::PrimitiveType::Integer),
                ),
            ),
        ],
    );
    let router = setup::setup_simple_auto_positive(
        json!({
            "missing_optional": null,
            "missing_list": [],
            "missing_set": [],
            "missing_map": {},
        }),
        endpoint_name,
        conjure_type,
    );

    run_test_case_against_server(&router, endpoint_name, None);
}

/// Test that a simple JSON round-trips against a mirroring server-under-test endpoint.
#[test]
fn test_returns_body() {
    let conjure_type = object_definition(
        "foo",
        &[field_definition(
            "heyo",
            primitive_type(ir::PrimitiveType::Integer),
        )],
    );
    let endpoint_name = "returns_body";
    let router =
        setup::setup_simple_auto_positive(json!({"heyo": 43}), endpoint_name, conjure_type);
    run_test_case_against_server(&router, endpoint_name, None);
}

/// Test that an empty optional sent as "null" accepts a 204 back.
#[test]
fn test_returns_204() {
    let conjure_type = optional_type(primitive_type(ir::PrimitiveType::String));
    let endpoint_name = "returns_204";
    let router = setup::setup_simple_auto_positive(json!(null), endpoint_name, conjure_type);
    run_test_case_against_server(&router, endpoint_name, None);
}

/// Spins up a server-under-test, and instructs the configured [VerificationClientResource]
/// identified by the given [Router] to run the given test case against it, making an assertion
/// on the error name (if any) returned by the call to the [VerificationClientResource].
fn run_test_case_against_server(
    router: &Router,
    endpoint_name: &str,
    expected_error: Option<&str>,
) {
    self::server_under_test::with_server_under_test(|addr| {
        let request = ClientRequest {
            endpoint_name: EndpointName::new(endpoint_name),
            test_case: 0,
            base_url: addr.to_string(),
        };
        setup::run_test_case(router, &request, |result| {
            match expected_error {
                Some(name) => assert_eq!(result.err().unwrap().name(), name),
                None => assert!(result.is_ok()),
            };
        });
    });
}

/// Convenient methods that construct [ResolvedType]s.
mod type_builders {
    use conjure::ir;
    use conjure::resolved_type::FieldDefinition;
    use conjure::resolved_type::ListType;
    use conjure::resolved_type::MapType;
    use conjure::resolved_type::ObjectDefinition;
    use conjure::resolved_type::OptionalType;
    use conjure::resolved_type::ResolvedType;
    use conjure::resolved_type::SetType;

    const PACKAGE: &'static str = "com.palantir.package";

    pub fn field_definition(field_name: &str, type_: ResolvedType) -> FieldDefinition {
        FieldDefinition {
            field_name: field_name.into(),
            type_,
        }
    }

    pub fn object_definition(name: &str, fields: &[FieldDefinition]) -> ResolvedType {
        ResolvedType::Object(ObjectDefinition {
            type_name: ir::TypeName {
                name: name.to_string(),
                package: PACKAGE.to_string(),
            },
            fields: fields.to_vec(),
        })
    }

    pub fn optional_type(item_type: ResolvedType) -> ResolvedType {
        ResolvedType::Optional(OptionalType {
            item_type: item_type.into(),
        })
    }

    pub fn list_type(item_type: ResolvedType) -> ResolvedType {
        ResolvedType::List(ListType {
            item_type: item_type.into(),
        })
    }

    pub fn set_type(item_type: ResolvedType) -> ResolvedType {
        ResolvedType::Set(SetType {
            item_type: item_type.into(),
        })
    }

    pub fn map_type(key_type: ir::PrimitiveType, value_type: ResolvedType) -> ResolvedType {
        ResolvedType::Map(MapType {
            key_type: key_type.into(),
            value_type: value_type.into(),
        })
    }

    pub fn primitive_type(primitive_type: ir::PrimitiveType) -> ResolvedType {
        ResolvedType::Primitive(primitive_type)
    }
}

/// Contains logic for setting up the [VerificationClientResource].
mod setup {
    use super::*;
    use conjure_verification_http_server::router::Binder;
    use typed_headers::{ContentType, HeaderMapExt};

    /// Simulate asking the VerificationClientService to run a test case against a server-under-test.
    pub(crate) fn run_test_case<F>(router: &Router, req: &ClientRequest, response_assertion: F)
    where
        F: FnOnce(Result<Response>),
    {
        if let RouteResult::Matched { endpoint, .. } = router.route(&Method::POST, "/runTestCase") {
            let mut builder = RequestBuilder::default();
            builder.headers.typed_insert(&ContentType(APPLICATION_JSON));
            builder.body = serde_json::to_vec(req).unwrap();
            let result: Result<Response> = builder.with_request(|req| endpoint.handler.handle(req));
            println!(
                "Got result error for request {:?}: {:?}",
                req,
                result.as_ref().err()
            );
            response_assertion(result);
        } else {
            panic!("Failed to route!")
        }
    }

    /// Sets up a [VerificationClientResource] with a single auto_deserialize positive test case for
    /// the given endpoint.
    pub fn setup_simple_auto_positive(
        test_body: serde_json::Value,
        endpoint_name: &str,
        conjure_type: ResolvedType,
    ) -> Router {
        setup_routes(|test_cases, param_types| {
            test_cases.auto_deserialize = hashmap!(
                    EndpointName::new(endpoint_name) => PositiveAndNegativeTestCases {
                        positive: vec![test_body.to_string()],
                        negative: vec![],
                    }
                );
            param_types.insert(EndpointName::new(endpoint_name), conjure_type);
        })
    }

    /// Sets up a router for a [VerificationClientResource] handling the desired server test cases.
    fn setup_routes<F>(f: F) -> Router
    where
        F: FnOnce(&mut ServerTestCases, &mut ParamTypes),
    {
        let mut test_cases = ServerTestCases::default();
        let mut param_types = HashMap::default();
        f(&mut test_cases, &mut param_types);
        let (router, _) = create_resource(test_cases, param_types);
        router
    }

    fn create_resource(
        test_cases: ServerTestCases,
        param_types: ParamTypes,
    ) -> (Router, Arc<VerificationClientResource>) {
        let resource = Arc::new(VerificationClientResource::new(
            Box::new(test_cases),
            Box::new(param_types),
        ));
        let mut builder = router::Router::builder();
        {
            let ref mut binder = Binder::new(resource.clone(), &mut builder, "");
            VerificationClientResource::register(binder);
        }
        (builder.build(), resource)
    }

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
}

/// Logic to set up and run actions against a mock server-under-test.
mod server_under_test {
    use super::*;
    use conjure_verification_http_server::handler::HttpService;
    use conjure_verification_http_server::router::Binder;
    use conjure_verification_http_server::DynamicResource;

    /// Spins up a server under test using [ServerUnderTest] on a random localhost port and returns
    /// the address where it was bound.
    pub fn with_server_under_test<F>(f: F)
    where
        F: FnOnce(Url),
    {
        use futures::{future, Future};
        use hyper;
        use tokio::runtime::Runtime;

        let addr = "127.0.0.1:0".parse().unwrap();
        let prefix = "server-under-test";

        let resource = Arc::new(ServerUnderTest {});
        let mut builder = router::Router::builder();
        {
            let ref mut binder = Binder::new(resource.clone(), &mut builder, prefix);
            binder.register_externally(DynamicResource::register);
        }

        let router = Arc::new(builder.build());

        let new_service = move || future::ok::<_, hyper::Error>(HttpService::new(router.clone()));
        let server0 = hyper::Server::bind(&addr);
        let server = server0.serve(new_service);
        let bound_addr = server.local_addr();

        let future = future::lazy(move || server.map_err(|e| eprintln!("server error: {}", e)));

        let mut runtime = Runtime::new().unwrap();
        runtime.spawn(future);
        println!("Started server under test at {}", bound_addr);

        let url: Url = format!(
            "http://{}:{}/{}",
            bound_addr.ip(),
            bound_addr.port(),
            prefix
        ).parse()
        .unwrap();
        f(url);

        runtime.shutdown_now().wait().unwrap();
    }

    struct ServerUnderTest;

    impl ServerUnderTest {
        /// Always returns a 204
        fn returns_204(&self, _request: &mut Request) -> Result<impl IntoResponse> {
            Ok(NoContent)
        }

        /// Always returns back the JSON body
        fn returns_body(&self, request: &mut Request) -> Result<impl IntoResponse> {
            let value: serde_json::Value = request.body()?;
            Ok(value)
        }

        /// Always returns the string "foo"
        fn returns_string_foo(&self, _request: &mut Request) -> Result<impl IntoResponse> {
            Ok(json!("foo"))
        }

        /// Always returns an empty json object
        fn returns_empty_object(&self, _request: &mut Request) -> Result<impl IntoResponse> {
            Ok(json!({}))
        }

        /// Returns an empty-content-type response of the int '10'. Used for validation testing.
        fn returns_non_json_10_body(&self, _request: &mut Request) -> Result<Response> {
            let mut response = Response::new(StatusCode::OK);
            response.body = Body::Fixed(Bytes::from("10"));
            Ok(response)
        }
    }

    impl Resource for ServerUnderTest {
        const BASE_PATH: &'static str = "";

        fn register<R>(_router: &mut R)
        where
            R: Route<Self>,
        {
        }
    }

    impl DynamicResource for ServerUnderTest {
        fn register<R>(&self, router: &mut R)
        where
            R: Route<Self>,
        {
            router.post("/returns_body", "", ServerUnderTest::returns_body);
            router.post("/returns_204", "", ServerUnderTest::returns_204);
            router.post(
                "/returns_string_foo",
                "",
                ServerUnderTest::returns_string_foo,
            );
            router.post(
                "/returns_empty_object",
                "",
                ServerUnderTest::returns_empty_object,
            );
            router.post(
                "/returns_non_json_10_body",
                "",
                ServerUnderTest::returns_non_json_10_body,
            );
        }
    }
}
