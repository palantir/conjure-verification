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
    //    pub types: Vec<IrType>,
    pub services: Vec<ServiceIr>,
}

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
pub enum ArgTypeIr {
    Primitive(PrimitiveType),
    Optional(Box<OptionalArgTypeIr>),
    Reference(ReferenceTypeIr),
}

#[derive(ConjureDeserialize, Debug)]
pub struct ReferenceTypeIr {
    name: String,
    package: String,
}

#[derive(ConjureDeserialize, Debug)]
pub struct OptionalArgTypeIr {
    pub item_type: ArgTypeIr,
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
        let _ir: Ir = serde_json::from_reader(file).unwrap();
    }
}
