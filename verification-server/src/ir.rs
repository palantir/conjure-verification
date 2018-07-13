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
    Object(TypeDefinitionBody<ObjectDefinition<Type>>),
    Alias(TypeDefinitionBody<AliasDefinition<Type>>),
    Enum(TypeDefinitionBody<EnumDefinition>),
    Union(TypeDefinitionBody<UnionDefinition<Type>>),
}

impl TypeDefinition {
    pub fn type_name<'a>(&'a self) -> &'a TypeName {
        match self {
            &TypeDefinition::Object(ref def) => &def.type_name,
            &TypeDefinition::Alias(ref def) => &def.type_name,
            &TypeDefinition::Enum(ref def) => &def.type_name,
            &TypeDefinition::Union(ref def) => &def.type_name,
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TypeDefinitionBody<T> {
    pub type_name: TypeName,
    #[serde(flatten)]
    pub definition: T,
}

#[derive(ConjureDeserialize, Debug)]
pub struct ObjectDefinition<Inner> {
    pub fields: Vec<FieldDefinition<Inner>>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct AliasDefinition<Inner> {
    pub alias: Box<Inner>,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct EnumDefinition {
    pub values: Vec<EnumValueDefinition>,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct EnumValueDefinition {
    pub value: String,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct UnionDefinition<Inner> {
    pub union: Vec<FieldDefinition<Inner>>,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub enum Type {
    Reference(TypeName),
    Primitive(PrimitiveType),
    Optional(OptionalType<Type>),
    List(ListType<Type>),
    Set(SetType<Type>),
    Map(MapType<Type, Type>),
}

#[derive(ConjureDeserialize, Debug, Clone, Eq, PartialEq)]
pub struct TypeName {
    pub name: String,
    pub package: String,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct OptionalType<Inner> {
    pub item_type: Box<Inner>,
}

impl<Inner> OptionalType<Inner> {
    pub fn new(t: Inner) -> OptionalType<Inner> {
        OptionalType {
            item_type: t.into(),
        }
    }
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
pub struct ListType<Inner> {
    pub item_type: Box<Inner>,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct SetType<Inner> {
    pub item_type: Box<Inner>,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct MapType<Key, Value> {
    pub key_type: Box<Key>,
    pub value_type: Box<Value>,
}

#[derive(ConjureDeserialize, Debug, Clone)]
pub struct FieldDefinition<Inner> {
    pub field_name: String,
    pub type_: Inner,
}

// Services

#[derive(ConjureDeserialize, Debug)]
pub struct ServiceDefinition {
    pub service_name: ServiceName,
    pub endpoints: Vec<EndpointDefinition>,
}

#[derive(ConjureDeserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct ServiceName {
    pub name: String,
    pub package: String,
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
    #[ignore] // test ignored because you need to `./gradlew compileTestCasesJson` first
    fn test() {
        let file = File::open(Path::new(
            "../verification-api/build/conjure-ir/verification-api.json",
        )).unwrap();
        let _ir: Conjure = serde_json::from_reader(file).unwrap();
    }
}
