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
pub struct Ir {
    pub types: Vec<TypeDeclarationIr>,
    pub services: Vec<ServiceIr>,
}

// Types

#[derive(ConjureDeserialize, Debug)]
pub enum TypeDeclarationIr {
    Object(ObjectTypeIr),
    Alias(AliasTypeIr),
    Enum(EnumTypeIr),
    Union(UnionTypeIr),
}

#[derive(ConjureDeserialize, Debug)]
pub struct ObjectTypeIr {
    type_name: TypeNameIr,
    fields: Vec<FieldIr>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct AliasTypeIr {
    pub type_name: TypeNameIr,
    pub alias: Box<TypeRefIr>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct EnumTypeIr {
    pub type_name: TypeNameIr,
    values: Vec<EnumValueIr>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct EnumValueIr {
    value: String,
}

#[derive(ConjureDeserialize, Debug)]
pub struct UnionTypeIr {
    pub type_name: TypeNameIr,
    // TODO
}

#[derive(ConjureDeserialize, Debug)]
pub enum TypeRefIr {
    Reference(TypeNameIr),
    Primitive(PrimitiveType),
    Optional(OptionalTypeIr),
    List(ListTypeIr),
    Set(SetTypeIr),
    Map(MapTypeIr),
}

#[derive(ConjureDeserialize, Debug)]
pub struct TypeNameIr {
    name: String,
    package: String,
}

#[derive(ConjureDeserialize, Debug)]
pub struct OptionalTypeIr {
    pub item_type: Box<TypeRefIr>,
}

#[derive(ConjureDeserialize, Debug)]
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

#[derive(ConjureDeserialize, Debug)]
pub struct ListTypeIr {
    item_type: Box<TypeRefIr>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct SetTypeIr {
    item_type: Box<TypeRefIr>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct MapTypeIr {
    key_type: Box<TypeRefIr>,
    value_type: Box<TypeRefIr>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct FieldIr {
    field_name: String,
    type_: TypeRefIr,
}

// Services

#[derive(ConjureDeserialize, Debug)]
pub struct ServiceIr {
    pub endpoints: Vec<EndpointIr>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct EndpointIr {
    pub endpoint_name: String,
    pub args: Vec<EndpointArgIr>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct EndpointArgIr {
    pub arg_name: String,
    pub type_: ArgTypeIr,
}

type ArgTypeIr = TypeRefIr;

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
        let _ir: Ir = serde_json::from_reader(file).unwrap();
    }
}
