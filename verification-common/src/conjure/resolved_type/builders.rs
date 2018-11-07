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

//! Convenient methods that construct [ResolvedType]s.

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

pub fn enum_definition(name: &str, variants: &[&str]) -> ResolvedType {
    ResolvedType::Enum(ir::EnumDefinition {
        type_name: type_name(name),
        values: variants
            .iter()
            .map(|value| ir::EnumValueDefinition {
                value: value.to_string(),
            }).collect(),
    })
}

pub fn type_name(name: &str) -> ir::TypeName {
    ir::TypeName {
        name: name.to_string(),
        package: PACKAGE.to_string(),
    }
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

pub fn map_type(key_type: ResolvedType, value_type: ResolvedType) -> ResolvedType {
    ResolvedType::Map(MapType {
        key_type: key_type.into(),
        value_type: value_type.into(),
    })
}

pub fn primitive_type(primitive_type: ir::PrimitiveType) -> ResolvedType {
    ResolvedType::Primitive(primitive_type)
}
