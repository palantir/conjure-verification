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

use ir::*;
use serde::Deserialize;
use type_resolution::ResolvedType;
use type_resolution::ResolvedType::*;
//use erased_serde::Deserializer;
use serde::{self, Deserializer};
use serde_json::{self, Value};
//use either::{Either, Left, Right};
use conjure_verification_error::{Code, Error};
use std::error;
use std::marker::PhantomData;

#[derive(Deserialize, PartialEq)]
struct BearerToken(String);

#[derive(Deserialize, PartialEq)]
struct Uuid(String);

#[derive(Deserialize, PartialEq)]
struct Rid(String);

#[allow(dead_code)]
pub fn equals<'de, D1, D2, E1, E2>(type_: &'de ResolvedType, a: D1, b: D2) -> Result<bool, Error>
where
    D1: Deserializer<'de, Error = E1>,
    D2: Deserializer<'de, Error = E2>,
    E1: serde::de::Error + Into<Box<error::Error + Sync + Send>>,
    E2: serde::de::Error + Into<Box<error::Error + Sync + Send>>,
{
    match type_ {
        Primitive(PrimitiveType::Bearertoken) => compare(a, b, PhantomData::<BearerToken>),
        Primitive(PrimitiveType::Uuid) => compare(a, b, PhantomData::<Uuid>),
        Primitive(PrimitiveType::Rid) => compare(a, b, PhantomData::<Rid>),

//        Primitive(PrimitiveType::Datetime) => compare(a, b, PhantomData::<??>), // TODO
        // TODO(dsanduleac): what type matches safelong?
        Primitive(PrimitiveType::Safelong) => compare(a, b, PhantomData::<u64>),
        Primitive(PrimitiveType::Integer) => compare(a, b, PhantomData::<u64>),
        Primitive(PrimitiveType::Double) => compare(a, b, PhantomData::<f64>),
        Primitive(PrimitiveType::String) => compare(a, b, PhantomData::<&str>),
        Primitive(PrimitiveType::Binary) => compare(a, b, PhantomData::<&[u8]>),
        Primitive(PrimitiveType::Boolean) => compare(a, b, PhantomData::<bool>),
        // Compare everything else as literal json value
        _ => compare(a, b, PhantomData::<Value>),
    }
}

fn deser_as<'de, T, D, E>(deserializer: D) -> Result<T, Error>
where
    D: Deserializer<'de, Error = E>,
    T: Deserialize<'de>,
    E: serde::de::Error + Into<Box<error::Error + Sync + Send>>,
{
    Deserialize::deserialize(deserializer).map_err(|e| Error::new_safe(e, Code::InvalidArgument))
}

fn compare<'de, T, D1, D2, E1, E2>(a: D1, b: D2, phantom: PhantomData<T>) -> Result<bool, Error>
where
    D1: Deserializer<'de, Error = E1>,
    D2: Deserializer<'de, Error = E2>,
    T: Deserialize<'de> + PartialEq,
    E1: serde::de::Error + Into<Box<error::Error + Sync + Send>>,
    E2: serde::de::Error + Into<Box<error::Error + Sync + Send>>,
{
    let left: T = deser_as(a)?;
    let right: T = deser_as(b)?;
    Ok(left == right)
}
