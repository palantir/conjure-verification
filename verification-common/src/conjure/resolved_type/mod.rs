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

use conjure::ir::EnumDefinition;
use conjure::ir::PrimitiveType;
use conjure::ir::TypeName;

pub mod builders;

/// Similar to the conjure::ir::Type, but doesn't have a `Reference` variant.
/// Instead, these are inlined.
#[derive(Debug, Clone)]
pub enum ResolvedType {
    // named types
    Object(ObjectDefinition),
    Enum(EnumDefinition),
    Union(UnionDefinition),

    // anonymous types
    Primitive(PrimitiveType),
    Optional(OptionalType),
    List(ListType),
    Set(SetType),
    Map(MapType),
}

#[derive(Debug, Clone)]
pub struct ListType {
    pub item_type: Box<ResolvedType>,
}

#[derive(Debug, Clone)]
pub struct SetType {
    pub item_type: Box<ResolvedType>,
}

#[derive(Debug, Clone)]
pub struct MapType {
    pub key_type: Box<ResolvedType>,
    pub value_type: Box<ResolvedType>,
}

#[derive(Debug, Clone)]
pub struct FieldDefinition {
    pub field_name: String,
    pub type_: ResolvedType,
}

#[derive(Debug, Clone)]
pub struct OptionalType {
    pub item_type: Box<ResolvedType>,
}

#[derive(Debug, Clone)]
pub struct ObjectDefinition {
    pub type_name: TypeName,
    pub fields: Vec<FieldDefinition>,
}

#[derive(Debug, Clone)]
pub struct UnionDefinition {
    pub type_name: TypeName,
    pub union: Vec<FieldDefinition>,
}
