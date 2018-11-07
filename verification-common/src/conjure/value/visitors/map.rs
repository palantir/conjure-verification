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

use conjure::ir::*;
use conjure::resolved_type::ResolvedType;
use conjure::value::de_plain::deserialize_plain_primitive;
use conjure::value::*;
use core::fmt;
use serde;
use serde::de::Error;
use serde::de::MapAccess;
use serde::de::Visitor;
use serde::Deserialize;
use serde::Deserializer;
use serde_json;
use std::collections::btree_map;
use std::collections::BTreeMap;

/// This visitor also supports being visited as an option using `Deserializer::deserialize_option`,
/// whereby it will return a default.
pub struct ConjureMapVisitor<'a> {
    pub key_type: &'a PrimitiveType,
    pub value_type: &'a ResolvedType,
}

impl<'de: 'a, 'a> Visitor<'de> for ConjureMapVisitor<'a> {
    type Value = BTreeMap<ConjurePrimitiveValue, ConjureValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("map")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(BTreeMap::new())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }

    fn visit_map<A>(self, mut items: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut result = BTreeMap::new();

        while let Some(key) = items.next_key_seed(MapKey(self.key_type))? {
            let value = items.next_value_seed(self.value_type)?;
            match result.entry(key) {
                btree_map::Entry::Occupied(entry) => {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate field `{}`",
                        serde_json::ser::to_string(entry.key()).unwrap()
                    )));
                }
                btree_map::Entry::Vacant(entry) => {
                    entry.insert(value);
                }
            }
        }
        Ok(result)
    }
}

/// A map key is a conjure [PrimitiveType] that should be deserialized only from a string
/// representation.
///
/// [PrimitiveType]: ../../../ir/enum.PrimitiveType.html
pub struct MapKey<'a>(&'a PrimitiveType);

impl<'de: 'a, 'a> DeserializeSeed<'de> for MapKey<'a> {
    type Value = ConjurePrimitiveValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        // Step 1. deserialize a string
        let str = <&'a str as Deserialize<'de>>::deserialize(deserializer)
            .map_err(|e| serde::de::Error::custom(e))?;

        // TODO(dsanduleac): we should support enums too, using deserialize_plain
        deserialize_plain_primitive(self.0, &str).map_err(|e| serde::de::Error::custom(e))
    }
}
