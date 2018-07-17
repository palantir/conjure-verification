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
use conjure::resolved_type::ResolvedType::*;
use conjure::value::visitors::option::ConjureOptionVisitor;
use conjure::value::*;
use core::fmt;
use itertools::Itertools;
use serde::de::Error;
use serde::de::MapAccess;
use serde::de::SeqAccess;
use serde::de::Unexpected;
use serde::de::Visitor;
use serde::private::de::size_hint;
use serde::Deserialize;
use serde::{self, Deserializer};
use serde_conjure::UnionTypeField;
use serde_json;
use std::collections::btree_map;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::error::Error as StdError;

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
        // Handle missing *required* fields (filter out fields which were optional)
        if !self.map.is_empty() {
            let keys = self.map
                .iter()
                .filter_map(|(k, v)| match v {
                    Optional(_) => None,
                    _ => Some(k),
                })
                .map(|k| format!("`{}`", k))
                .join(", ");
            if keys.is_empty() {
                // Only optional fields. Set their values to None
                for (k, _) in self.map.into_iter() {
                    result.insert(k.to_string(), ConjureValue::Optional(None));
                }
            } else {
                return Err(serde::de::Error::custom(format_args!(
                    "missing fields: {}",
                    keys
                )));
            }
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
