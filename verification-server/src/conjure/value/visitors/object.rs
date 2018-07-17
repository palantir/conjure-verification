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

pub use serde::de::DeserializeSeed;

use conjure::ir::*;
use conjure::resolved_type::ResolvedType;
use conjure::value::*;
use core::fmt;
use itertools::Itertools;
use serde;
use serde::de::Error;
use serde::de::MapAccess;
use serde::de::Visitor;
use std::collections::BTreeMap;
use std::collections::HashMap;
use serde::Deserializer;
use std::marker::PhantomData;

pub struct ConjureObjectVisitor<'a> {
    pub map: HashMap<&'a str, &'a ResolvedType>,
    pub skip_unknown: bool,
}

impl<'a> ConjureObjectVisitor<'a> {
    pub fn new(
        fields: &'a Vec<FieldDefinition<ResolvedType>>,
        skip_unknown: bool,
    ) -> ConjureObjectVisitor {
        ConjureObjectVisitor {
            map: fields
                .iter()
                .map(|FieldDefinition { field_name, type_ }| (&**field_name, type_))
                .collect(),
            skip_unknown,
        }
    }
}

impl<'de: 'a, 'a> Visitor<'de> for ConjureObjectVisitor<'a> {
    type Value = BTreeMap<String, ConjureValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("struct")
    }

    fn visit_map<A>(mut self, mut items: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let known_fields = self.map.keys().cloned().collect();
        let mut result = BTreeMap::new();
        // Note: must deserialize String, not &str, because `de::Deserializer<'de> for
        // ::serde_json::value::Value` calls `visit_string` on our visitor, and the visitor used by
        // `de::Deserialize for String` (::serde::de::impls::StrVisitor) can't handle that.
        while let Some(key) = items.next_key::<String>()? {
            let field_type = self.map.remove(key.as_str());
            if let Some(field_type) = field_type {
                let value = items.next_value_seed(field_type)?;
                if let Some(_) = result.insert(key.to_string(), value) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate field `{}`",
                        key
                    )));
                }
            } else if !self.skip_unknown {
                return Err(unknown_field(&key.to_string(), known_fields));
            }
        }
        // Handle missing fields.
        for (field_name, field_type) in self.map {
            let deserializer = MissingFieldDeserializer(field_name, PhantomData);
            // This will succeed with an appropriate default value if the field type defines such
            // a default value (namely - its visitor accepts `visit_none` to indicate an explicit
            // value was missing), or otherwise fail with a 'missing field' error.
            let value = field_type.deserialize(deserializer)?;
            result.insert(field_name.to_string(), value);
        }
        Ok(result)
    }
}

/// Shameless kinda copied from serde::de::Error::unknown_field because they only take static strings.
fn unknown_field<'a, E: Error>(field: &'a str, expected: Vec<&'a str>) -> E {
    if expected.is_empty() {
        Error::custom(format_args!(
            "unknown field `{}`, there are no fields",
            field
        ))
    } else {
        Error::custom(format_args!(
            "unknown field `{}`, expected one of: {}",
            field,
            expected.into_iter().join(", ")
        ))
    }
}

/// A Deserializer for a specific field whose value was not present in the map.
///
/// When asked to deserialize an option, seq or map, it will `visit_none` on the visitor (because we
/// expect the Conjure visitors to handle these cases with default values), but will fail with a
/// missing field exception otherwise.
struct MissingFieldDeserializer<'a, E>(&'a str, PhantomData<E>);

impl<'de : 'a, 'a, E> Deserializer<'de> for MissingFieldDeserializer<'a, E> where E : Error {
    type Error = E;

    fn deserialize_any<V>(self, _: V) -> Result<V::Value, Self::Error> where
        V: Visitor<'de> {
        Err(Error::custom(format_args!("Missing field: {}", self.0)))
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
        V: Visitor<'de> {
        visitor.visit_none()
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
        V: Visitor<'de> {
        visitor.visit_none()
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
        V: Visitor<'de> {
        visitor.visit_none()
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct newtype_struct tuple enum
        tuple_struct struct identifier ignored_any
    }
}
