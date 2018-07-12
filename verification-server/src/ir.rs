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

#[derive(ConjureDeserialize, Debug)]
pub struct Conjure {
    pub types: Vec<TypeDefinition>,
    pub services: Vec<ServiceDefinition>,
}

// Types

#[derive(ConjureDeserialize, Debug)]
pub enum TypeDefinition {
    Object(ObjectDefinition),
    Alias(AliasDefinition),
    Enum(EnumDefinition),
    Union(UnionDefintion),
}

#[derive(ConjureDeserialize, Debug)]
pub struct ObjectDefinition {
    pub type_name: TypeName,
    pub fields: Vec<FieldDefinition>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct AliasDefinition {
    pub type_name: TypeName,
    pub alias: Box<Type>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct EnumDefinition {
    pub type_name: TypeName,
    pub values: Vec<EnumValueDefinition>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct EnumValueDefinition {
    pub value: String,
}

#[derive(ConjureDeserialize, Debug)]
pub struct UnionDefintion {
    pub type_name: TypeName,
    pub union: Vec<FieldDefinition>,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub enum Type {
    Reference(TypeName),
    Primitive(PrimitiveType),
    Optional(OptionalType),
    List(ListType),
    Set(SetType),
    Map(MapType),
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct TypeName {
    pub name: String,
    pub package: String,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct OptionalType {
    pub item_type: Box<Type>,
}

#[derive(ConjureDeserialize, Debug, Clone)]
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

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct ListType {
    pub item_type: Box<Type>,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct SetType {
    pub item_type: Box<Type>,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct MapType {
    pub key_type: Box<Type>,
    pub value_type: Box<Type>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct FieldDefinition {
    pub field_name: String,
    pub type_: Type,
}

// Services

#[derive(ConjureDeserialize, Debug)]
pub struct ServiceDefinition {
    pub endpoints: Vec<EndpointDefinition>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct EndpointDefinition {
    pub endpoint_name: String,
    pub args: Vec<ArgumentDefinition>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct ArgumentDefinition {
    pub arg_name: String,
    pub type_: Type,
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json;
    use std::fs::File;
    use std::path::Path;

    #[test]
    fn test() {
        let file = File::open(Path::new(
            "../verification-api/build/conjure-ir/verification-api.json",
        )).unwrap();
        let _ir: Conjure = serde_json::from_reader(file).unwrap();
    }
}
