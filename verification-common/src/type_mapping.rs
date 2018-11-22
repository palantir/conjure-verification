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

//! Given a [Conjure IR], derives a mapping for services we care about from
//! endpoint name to type of expected argument.
//! This is used in [SpecTestResource] to create typed deserializations into [ConjureValue].
//!
//! [ConjureValue]: ../conjure_serde_value/enum.ConjureValue.html
//! [SpecTestResource]: ../resource/struct.SpecTestResource.html
//! [Conjure IR]: ../ir/struct.Conjure.html

use conjure::ir;
use conjure::ir::Conjure;
use conjure::resolved_type::ResolvedType;
use conjure::type_resolution::resolve_type;
use std::collections::HashMap;
use test_spec::EndpointName;

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
/// The types of tests that you can run.
pub enum TestType {
    Body,
    SinglePathParam,
    SingleQueryParam,
    SingleHeaderParam,
}

#[derive(new)]
/// Defines that a service with a given name implements tests of the given [TestType], and
/// the Conjure type can be extracted from the endpoint definition using the given [TypeForEndpointFn].
///
/// [TestType]: enum.TestType.html
/// [TypeForEndpointFn]: type.TypeForEndpointFn.html
pub struct ServiceTypeMapping<'a> {
    pub service_name: &'a str,
    pub test_type: TestType,
    pub type_for_endpoint_fn: TypeForEndpointFn,
}

/// Type alias describing the mapping from test type -> endpoint name -> a conjure type.
pub type ParamTypes = HashMap<TestType, HashMap<EndpointName, ResolvedType>>;

pub fn resolve_types<'a, 'b>(
    ir: &'a Conjure,
    type_by_service: &'a [ServiceTypeMapping<'b>],
) -> ParamTypes {
    // Resolve endpoint -> type mappings eagerly
    let mut param_types = HashMap::new();
    type_by_service.iter().for_each(
        |ServiceTypeMapping {
             test_type,
             service_name,
             type_for_endpoint_fn,
         }| {
            if let Some(service) = ir
                .services
                .iter()
                .find(|service| service.service_name.name == *service_name)
            {
                let mut endpoint_map = HashMap::new();
                for e in &service.endpoints {
                    // Resolve aliases
                    let type_ = resolve_type(&ir.types, type_for_endpoint_fn(&e));
                    // Create a unique map
                    assert!(
                        endpoint_map
                            .insert(e.endpoint_name.clone().into(), type_)
                            .is_none()
                    );
                }
                assert!(
                    param_types
                        .insert(test_type.clone(), endpoint_map)
                        .is_none()
                );
            } else {
                panic!("Unable to find matching service for {}", service_name);
            }
        },
    );
    param_types
}

pub fn type_of_non_index_arg(endpoint_def: &ir::EndpointDefinition) -> &ir::Type {
    &endpoint_def
        .args
        .iter()
        .find(|arg| arg.arg_name != "index")
        .unwrap()
        .type_
}

pub fn return_type(endpoint_def: &ir::EndpointDefinition) -> &ir::Type {
    (&endpoint_def.returns).as_ref().unwrap()
}

pub type TypeForEndpointFn = fn(&ir::EndpointDefinition) -> &ir::Type;

/// Builder for easy construction of of the return type of [resolve_types](fn.resolve_types.html).
pub mod builder {
    use super::*;

    type ParamTypes = HashMap<TestType, HashMap<EndpointName, ResolvedType>>;

    #[derive(Default)]
    pub struct ParamTypesBuilder(ParamTypes);

    impl ParamTypesBuilder {
        pub fn add(
            &mut self,
            tt: TestType,
            endpoint_name: EndpointName,
            resolved_type: ResolvedType,
        ) -> &mut Self {
            self.0
                .entry(tt)
                .or_default()
                .insert(endpoint_name, resolved_type);
            self
        }

        pub fn build(self) -> ParamTypes {
            self.0
        }
    }
}
