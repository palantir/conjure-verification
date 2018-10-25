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
use conjure::ir::ServiceName;
use conjure::resolved_type::ResolvedType;
use conjure::type_resolution::resolve_type;
use std::collections::HashMap;
use test_spec::EndpointName;

pub fn resolve_types(ir: &Conjure, type_by_service: &HashMap<ServiceName, TypeForEndpointFn>) -> Box<HashMap<EndpointName, ResolvedType>> {

    // Resolve endpoint -> type mappings eagerly
    let mut param_types = Box::new(HashMap::new());
    ir.services
        .iter()
        // TODO throw if service not found
        .filter_map(|s| type_by_service.get(&s.service_name).map(|func| (s, func)))
        .for_each(|(s, func)| {
            for e in &s.endpoints {
                // Resolve aliases
                let type_ = resolve_type(&ir.types, func(&e));
                // Create a unique map
                assert!(
                    param_types
                        .insert(e.endpoint_name.clone().into(), type_)
                        .is_none()
                );
            }
        });
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
