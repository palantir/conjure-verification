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

//! Similar to serde_json::value::Value, this represents a runtime type hierarchy for anonymous
//! objects deserialized from Conjure JSON.
//!
//! This is not expected to be used outside the conjure-verification project because users would
//! have real, named Conjure-generated types to deserialize into.

pub use serde::de::DeserializeSeed;

use self::double::ConjureDouble;
use chrono::DateTime;
use chrono::FixedOffset;
use serde_value::Value;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use uuid::Uuid;

pub mod de;
pub mod de_plain;
pub mod double;
mod util;
mod visitors;

#[derive(ConjureSerialize, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum ConjurePrimitiveValue {
    String(String),
    Integer(i32),
    Double(ConjureDouble),
    Boolean(bool),
    /// Integer with value ranging from -2^53 - 1 to 2^53 - 1 // TODO enforce?
    Safelong(i64),
    Binary(Binary),
    Uuid(Uuid),
    // TODO(dsanduleac): own type
    Rid(String),
    // TODO(dsanduleac): own type
    Bearertoken(String),
    Datetime(DateTime<FixedOffset>),
    Any(Value), // just use Value for any
}

impl ConjurePrimitiveValue {
    /// Convenience method.
    pub fn double(d: f64) -> ConjurePrimitiveValue {
        ConjurePrimitiveValue::Double(ConjureDouble::new(d))
    }
}

#[derive(ConjureSerialize, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum ConjureValue {
    Primitive(ConjurePrimitiveValue),
    // complex
    Optional(Option<Box<ConjureValue>>),
    Object(BTreeMap<String, ConjureValue>),
    Enum(String),
    Union(ConjureUnionValue),
    // anonymous
    List(Vec<ConjureValue>),
    Set(BTreeSet<ConjureValue>),
    Map(BTreeMap<ConjureValue, ConjureValue>),
}

#[derive(ConjureSerialize, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct ConjureUnionValue {
    pub field_name: String,
    pub value: Box<ConjureValue>,
}

/// Deserialized only from a base-64 encoded string.
#[derive(Serialize, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct Binary(Vec<u8>);
