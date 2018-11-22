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

//! These types are sufficient to deserialize [Conjure's IR][ir], apart from 'errors'.
//! Long-term, we intend these to live in a standalone conjure-rust project.
//!
//! [ir]: https://github.com/palantir/conjure/blob/develop/docs/intermediate_representation.md

#[derive(ConjureDeserialize, ConjureSerialize, Debug, PartialEq, Eq)]
pub struct Conjure {
    pub types: Vec<TypeDefinition>,
    pub services: Vec<ServiceDefinition>,
}

// Types

#[derive(ConjureDeserialize, ConjureSerialize, Debug, PartialEq, Eq)]
pub enum TypeDefinition {
    Object(ObjectDefinition),
    Alias(AliasDefinition),
    Enum(EnumDefinition),
    Union(UnionDefinition),
}

impl TypeDefinition {
    pub fn type_name(&self) -> &TypeName {
        match *self {
            TypeDefinition::Object(ref def) => &def.type_name,
            TypeDefinition::Alias(ref def) => &def.type_name,
            TypeDefinition::Enum(ref def) => &def.type_name,
            TypeDefinition::Union(ref def) => &def.type_name,
        }
    }
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, PartialEq, Eq)]
pub struct ObjectDefinition {
    pub type_name: TypeName,
    pub fields: Vec<FieldDefinition>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, PartialEq, Eq)]
pub struct AliasDefinition {
    pub type_name: TypeName,
    pub alias: Box<Type>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, PartialEq, Eq)]
pub struct EnumDefinition {
    pub type_name: TypeName,
    pub values: Vec<EnumValueDefinition>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, PartialEq, Eq)]
pub struct EnumValueDefinition {
    pub value: String,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, PartialEq, Eq)]
pub struct UnionDefinition {
    pub type_name: TypeName,
    pub union: Vec<FieldDefinition>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Reference(TypeName),
    Primitive(PrimitiveType),
    Optional(OptionalType),
    List(ListType),
    Set(SetType),
    Map(MapType),
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct TypeName {
    pub name: String,
    pub package: String,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, PartialEq, Eq)]
pub struct OptionalType {
    pub item_type: Box<Type>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, PartialEq, Eq)]
pub enum PrimitiveType {
    String,
    Integer,
    Double,
    Boolean,
    Safelong,
    Binary,
    Uuid,
    Rid,
    Bearertoken,
    Datetime,
    Any,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, PartialEq, Eq)]
pub struct ListType {
    pub item_type: Box<Type>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, PartialEq, Eq)]
pub struct SetType {
    pub item_type: Box<Type>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, PartialEq, Eq)]
pub struct MapType {
    pub key_type: Box<Type>,
    pub value_type: Box<Type>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, PartialEq, Eq)]
pub struct FieldDefinition {
    pub field_name: String,
    pub type_: Box<Type>,
}

// Services

#[derive(ConjureDeserialize, ConjureSerialize, Debug, PartialEq, Eq)]
pub struct ServiceDefinition {
    pub service_name: ServiceName,
    pub endpoints: Vec<EndpointDefinition>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct ServiceName {
    pub name: String,
    pub package: String,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, PartialEq, Eq)]
pub struct EndpointDefinition {
    pub endpoint_name: String,
    pub args: Vec<ArgumentDefinition>,
    pub returns: Option<Type>,
}

#[derive(ConjureDeserialize, ConjureSerialize, Debug, PartialEq, Eq)]
pub struct ArgumentDefinition {
    pub arg_name: String,
    pub type_: Type,
}
