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

use chrono::DateTime;
use chrono::FixedOffset;
use conjure::safe_double::SafeDouble;
use core::fmt;
use ir::*;
use itertools::Itertools;
use serde::de::Error;
use serde::de::MapAccess;
use serde::de::SeqAccess;
use serde::de::Visitor;
use serde::private::de::size_hint;
use serde::Deserialize;
use serde::{self, Deserializer};
use serde_json;
use serde_value::Value;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::error::Error as StdError;
use type_resolution::ResolvedType;
use type_resolution::ResolvedType::*;
use uuid::Uuid;
use std::collections::BTreeSet;
use std::collections::btree_map;

#[derive(ConjureSerialize, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum ConjurePrimitiveValue {
    String(String),
    Integer(i32),
    Double(SafeDouble),
    Boolean(bool),
    /// Integer with value ranging from -2^53 - 1 to 2^53 - 1 // TODO enforce?
    Safelong(i64),
    Binary(Vec<u8>),
    Uuid(Uuid),
    Rid(String),         // TODO
    Bearertoken(String), // TODO
    Datetime(DateTime<FixedOffset>),
    Any(Value), // just use Value for any
}

impl ConjurePrimitiveValue {
    /// Convenience method. Panics if `d` is `NaN`.
    pub fn double(d: f64) -> ConjurePrimitiveValue {
        ConjurePrimitiveValue::Double(SafeDouble::new(d).unwrap())
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
    Map(BTreeMap<ConjurePrimitiveValue, ConjureValue>),
}

#[derive(ConjureSerialize, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct ConjureUnionValue {
    pub field_name: String,
    pub value: Box<ConjureValue>,
}

/// Allows you to deserialize a given type without having to type it.
trait DeserQuick<'de> {
    type Error;
    fn deser<T>(self) -> Result<T, Self::Error>
    where
        T: Deserialize<'de>;
}

impl<'de, D> DeserQuick<'de> for D
where
    D: Deserializer<'de>,
{
    type Error = D::Error;

    fn deser<T>(self) -> Result<T, Self::Error>
    where
        T: Deserialize<'de>,
    {
        T::deserialize(self)
    }
}

// Visitors!!!

struct OptionVisitor<'a>(&'a ResolvedType);

impl<'de: 'a, 'a> Visitor<'de> for OptionVisitor<'a> {
    type Value = Option<ConjureValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("option")
    }

    #[inline]
    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: StdError,
    {
        Ok(None)
    }

    #[inline]
    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        self.0.deserialize(deserializer).map(Some)
    }

    #[inline]
    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: StdError,
    {
        Ok(None)
    }
}

struct ObjectVisitor<'a> {
    map: HashMap<&'a str, &'a ResolvedType>,
    skip_unknown: bool,
}

impl<'a> ObjectVisitor<'a> {
    fn new(fields: &'a Vec<FieldDefinition<ResolvedType>>, skip_unknown: bool) -> ObjectVisitor {
        ObjectVisitor {
            map: fields
                .iter()
                .map(|FieldDefinition { field_name, type_ }| (&**field_name, type_))
                .collect(),
            skip_unknown,
        }
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

impl<'de: 'a, 'a> Visitor<'de> for ObjectVisitor<'a> {
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

struct SeqVisitor<'a>(&'a ResolvedType);

impl<'de: 'a, 'a> Visitor<'de> for SeqVisitor<'a> {
    type Value = Vec<ConjureValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("list")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(size_hint::cautious(seq.size_hint()));

        while let Some(value) = seq.next_element_seed(self.0)? {
            values.push(value);
        }

        Ok(values)
    }
}

struct SetVisitor<'a> {
    item_type: &'a ResolvedType,
    fail_on_duplicates: bool,
}

impl<'de: 'a, 'a> Visitor<'de> for SetVisitor<'a> {
    type Value = BTreeSet<ConjureValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a sequence")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where
        A: SeqAccess<'de>, {
        let mut values = BTreeSet::new();

        while let Some(value) = seq.next_element_seed(self.item_type)? {
            if self.fail_on_duplicates && values.contains(&value) {
                return Err(Error::custom(format_args!(
                    "Set contained duplicates: {}",
                    serde_json::ser::to_string(&value).unwrap())))
            }
            values.insert(value);
        }

        Ok(values)
    }
}

struct MapVisitor<'a> {
    key_type: &'a PrimitiveType,
    value_type: &'a ResolvedType,
}

impl<'de: 'a, 'a> Visitor<'de> for MapVisitor<'a> {
    type Value = BTreeMap<ConjurePrimitiveValue, ConjureValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("map")
    }

    fn visit_map<A>(self, mut items: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
    {
        let mut result = BTreeMap::new();
        while let Some(key) = items.next_key_seed(self.key_type)? {
            let value = items.next_value_seed(self.value_type)?;
            match result.entry(key) {
                btree_map::Entry::Occupied(entry) => {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate field `{}`",
                        serde_json::ser::to_string(entry.key()).unwrap()
                    )))
                },
                btree_map::Entry::Vacant(entry) => {
                    entry.insert(value);
                }
            }
        }
        Ok(result)
    }
}

impl<'de: 'a, 'a> DeserializeSeed<'de> for &'a PrimitiveType {
    type Value = ConjurePrimitiveValue;

    fn deserialize<D>(self, de: D) -> Result<Self::Value, <D as Deserializer<'de>>::Error>
        where
            D: Deserializer<'de>,
    {
        let out = match *self {
            PrimitiveType::Safelong => ConjurePrimitiveValue::Safelong(de.deser()?),
            PrimitiveType::Integer => ConjurePrimitiveValue::Integer(de.deser()?),
            PrimitiveType::Double => ConjurePrimitiveValue::Double(de.deser()?),
            PrimitiveType::String => ConjurePrimitiveValue::String(de.deser()?),
            PrimitiveType::Binary => ConjurePrimitiveValue::Binary(de.deser()?),
            PrimitiveType::Boolean => ConjurePrimitiveValue::Boolean(de.deser()?),
            PrimitiveType::Uuid => ConjurePrimitiveValue::Uuid(de.deser()?),
            PrimitiveType::Rid => ConjurePrimitiveValue::Rid(de.deser()?),
            PrimitiveType::Bearertoken => ConjurePrimitiveValue::Bearertoken(de.deser()?),
            PrimitiveType::Datetime => ConjurePrimitiveValue::Datetime(de.deser()?),
            PrimitiveType::Any => ConjurePrimitiveValue::Any(de.deser()?),
        };
        Ok(out)
    }
}

impl<'de: 'a, 'a> DeserializeSeed<'de> for &'a ResolvedType {
    type Value = ConjureValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match self {
            Primitive(p) => ConjureValue::Primitive(p.deserialize(deserializer)?),
            Optional(OptionalType { item_type }) => ConjureValue::Optional(
                deserializer
                    .deserialize_option(OptionVisitor(&item_type))?
                    .map(Box::new),
            ),
            Object(ObjectDefinition { fields }) => {
                // TODO(dsanduleac): bubble up the skip_unknown (it's false for servers)
                ConjureValue::Object(
                    deserializer.deserialize_map(ObjectVisitor::new(&fields, false))?
                )
            }
            List(ListType { item_type }) => {
                ConjureValue::List(deserializer.deserialize_seq(SeqVisitor(&item_type))?)
            },
            Set(SetType { ref item_type }) => {
                ConjureValue::Set(deserializer.deserialize_seq(SetVisitor {
                    item_type,
                    fail_on_duplicates: false,
                })?)
            },
            Map(MapType { ref key_type, ref value_type }) => {
                ConjureValue::Map(deserializer.deserialize_map(MapVisitor {
                    key_type,
                    value_type,
                })?)
            },
            _ => unimplemented!(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json_2::*;
    use serde_plain;

    #[test]
    fn test_double() {
        let type_ = ResolvedType::Primitive(PrimitiveType::Double);
        assert_eq!(
            from_str(&type_, "123").unwrap(),
            ConjureValue::Primitive(ConjurePrimitiveValue::double(123.0))
        );
        assert!(from_str(&type_, "").is_err());
        assert!(from_str(&type_, "null").is_err());
    }

    #[test]
    fn test_optional() {
        let type_ = ResolvedType::Optional(OptionalType {
            item_type: ResolvedType::Primitive(PrimitiveType::Double).into(),
        });
        assert_eq!(
            from_str(&type_, "123").unwrap(),
            ConjureValue::Optional(Some(
                ConjureValue::Primitive(ConjurePrimitiveValue::double(123.0)).into()
            ))
        );
        assert_eq!(
            from_str(&type_, "null").unwrap(),
            ConjureValue::Optional(None)
        );
    }

    #[test]
    fn test_object_optional_fields() {
        let double_type = || ResolvedType::Primitive(PrimitiveType::Double);
        let type_ = ResolvedType::Object(ObjectDefinition {
            fields: vec![
                FieldDefinition {
                    field_name: "foo".to_string(),
                    type_: double_type(),
                },
                FieldDefinition {
                    field_name: "bar".to_string(),
                    type_: ResolvedType::Optional(OptionalType {
                        item_type: double_type().into(),
                    }),
                },
            ],
        });

        // Accepts missing optional fields
        assert_eq!(
            from_str(&type_, r#"{"foo": 123}"#).unwrap(),
            ConjureValue::Object(btreemap!(
                "foo" => ConjureValue::Primitive(ConjurePrimitiveValue::double(123.0)),
                "bar" => ConjureValue::Optional(None)
            ))
        );

        // Does not tolerate null for required field
        assert!(from_str(&type_, r#"{"foo": null}"#).is_err());

        // Tolerates null for optional field
        assert_eq!(
            from_str(&type_, r#"{"bar": null, "foo": 123}"#).unwrap(),
            ConjureValue::Object(btreemap!(
                "foo" => ConjureValue::Primitive(ConjurePrimitiveValue::double(123.0)),
                "bar" => ConjureValue::Optional(None)
            ))
        );

        // Deserializes present optional fields
        assert_eq!(
            from_str(&type_, r#"{"bar": 555, "foo": 123}"#).unwrap(),
            ConjureValue::Object(btreemap!(
                "foo" => ConjureValue::Primitive(ConjurePrimitiveValue::double(123.0)),
                "bar" => ConjureValue::Optional(Some(
                    ConjureValue::Primitive(ConjurePrimitiveValue::double(555.0)).into()))
            ))
        );

        // Fails on unknown fields (default = server implementation)
        assert!(from_str(&type_, r#"{"foo": 123, "whoami": 1}"#).is_err());

        // Fails on missing required field
        assert!(from_str(&type_, r#"{}"#).is_err());
    }

    /// Testing that we can use serde_plain::Deserializer with our DeserializeSeed implementation.
    #[test]
    fn deser_from_string() {
        let de = serde_plain::Deserializer::from_str("123");
        let typ = ResolvedType::Primitive(PrimitiveType::Double);
        assert_eq!(
            typ.deserialize(de).unwrap(),
            ConjureValue::Primitive(ConjurePrimitiveValue::double(123.0))
        );
    }
}
