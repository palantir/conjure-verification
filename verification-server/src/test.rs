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

use conjure::ir;
use conjure::resolved_type::FieldDefinition;
use conjure::resolved_type::ObjectDefinition;
use conjure::resolved_type::OptionalType;
use hyper::header::HeaderValue;
use hyper::HeaderMap;
use hyper::Method;
use mime::APPLICATION_JSON;
use register_resource;
use conjure::resolved_type::ResolvedType;
use conjure_verification_error::Result;
use conjure_verification_error::{Code};
use conjure_verification_http::request::Request;
use conjure_verification_http::response::Response;
use std::collections::HashMap;
use std::string::ToString;
use test_spec::ClientTestCases;
use test_spec::EndpointName;
use test_spec::PositiveAndNegativeTestCases;
use router;
use router::RouteResult;
use router::Router;
use std::sync::Arc;
use typed_headers::{ContentType, HeaderMapExt};

use resource::*;

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
        types.insert(
            EndpointName::new("string"),
            ResolvedType::Primitive(ir::PrimitiveType::String),
        );
        types.insert(
            EndpointName::new("int"),
            ResolvedType::Primitive(ir::PrimitiveType::Integer),
        );
        types.insert(
            EndpointName::new("bool"),
            ResolvedType::Primitive(ir::PrimitiveType::Boolean),
        );
        types.insert(
            EndpointName::new("opt"),
            ResolvedType::Optional(OptionalType {
                item_type: ResolvedType::Primitive(ir::PrimitiveType::Any).into(),
            }),
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
    let router = setup_routes(|cases, types| {
        cases.single_query_param_service = hashmap!(
                EndpointName::new("string") => vec!["\"yo\"".into()],
                EndpointName::new("int") => vec!["-1234".into()],
                EndpointName::new("bool") => vec!["false".into()],
                EndpointName::new("opt") => vec!["null".into()]
            );
        types.insert(
            EndpointName::new("string"),
            ResolvedType::Primitive(ir::PrimitiveType::String),
        );
        types.insert(
            EndpointName::new("int"),
            ResolvedType::Primitive(ir::PrimitiveType::Integer),
        );
        types.insert(
            EndpointName::new("bool"),
            ResolvedType::Primitive(ir::PrimitiveType::Boolean),
        );
        types.insert(
            EndpointName::new("opt"),
            ResolvedType::Optional(OptionalType {
                item_type: ResolvedType::Primitive(ir::PrimitiveType::Any).into(),
            }),
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
    if let RouteResult::Matched { endpoint, .. } = router.route(&Method::POST, "/confirm/foo/0") {
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
        panic!("Failed to route!")
    }
}

fn field_definition(field_name: &str, type_: ResolvedType) -> FieldDefinition {
    FieldDefinition {
        field_name: field_name.into(),
        type_,
    }
}

/// Sets up a router handling the desired client test cases.
fn setup_routes<F>(f: F) -> Router
where
    F: FnOnce(&mut ClientTestCases, &mut ParamTypes),
{
    let mut test_cases = ClientTestCases::default();
    let mut param_types = HashMap::default();
    f(&mut test_cases, &mut param_types);
    let (router, _) = create_resource(test_cases, param_types);
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
    let param_types = hashmap![
            EndpointName::new("foo") => ResolvedType::Object(ObjectDefinition {
                type_name: ir::TypeName { name: "Name".to_string(), package: "com.palantir.package".to_string() },
                fields: vec![
                    field_definition(
                        "heyo",
                        ResolvedType::Primitive(ir::PrimitiveType::Integer)
                    )
                ]
            })
        ];
    let (router, resource) = create_resource(test_cases, param_types);
    (expected_body, router, resource)
}

fn create_resource(
    test_cases: ClientTestCases,
    param_types: ParamTypes,
) -> (Router, Arc<SpecTestResource>) {
    let mut builder = router::Router::builder();
    let resource = Arc::new(SpecTestResource::new(
        Box::new(test_cases),
        Box::new(param_types),
    ));
    register_resource(&mut builder, &resource);
    (builder.build(), resource)
}
