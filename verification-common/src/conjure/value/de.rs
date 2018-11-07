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

//! This file providers trait implementations so that you can convert a
//!
//! conjure::resolved_type::ResolvedType into a conjure::value::ConjureValue
//!
//! Note, there is no internal mutability here, we just use DeserializeSeed to pass information into
//! the deserialization process (contextual deserialization) instead of the usual context-free
//! deserialization.

pub use serde::de::DeserializeSeed;

use super::*;
use conjure::ir::EnumDefinition;
use conjure::ir::PrimitiveType;
use conjure::resolved_type::ResolvedType::*;
use conjure::resolved_type::*;
use conjure::value::util::unknown_variant;
use conjure::value::visitors::map::ConjureMapVisitor;
use conjure::value::visitors::object::ConjureObjectVisitor;
use conjure::value::visitors::option::ConjureOptionVisitor;
use conjure::value::visitors::seq::ConjureSeqVisitor;
use conjure::value::visitors::set::ConjureSetVisitor;
use conjure::value::visitors::union::ConjureUnionVisitor;
use core::fmt;
use serde::de::Error;
use serde::de::Visitor;
use serde::Deserialize;
use serde::Deserializer;

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
                    .deserialize_option(ConjureOptionVisitor(&item_type))?
                    .map(Box::new),
            ),
            Object(ObjectDefinition { fields, .. }) => {
                // TODO(dsanduleac): bubble up the skip_unknown (it's false for servers)
                ConjureValue::Object(
                    deserializer.deserialize_map(ConjureObjectVisitor::new(&fields, false))?,
                )
            }
            List(ListType { item_type }) => {
                ConjureValue::List(deserializer.deserialize_seq(ConjureSeqVisitor(&item_type))?)
            }
            Set(SetType { ref item_type }) => {
                ConjureValue::Set(deserializer.deserialize_seq(ConjureSetVisitor {
                    item_type,
                    fail_on_duplicates: true,
                })?)
            }
            Map(MapType {
                ref key_type,
                ref value_type,
            }) => ConjureValue::Map(deserializer.deserialize_map(ConjureMapVisitor {
                key_type,
                value_type,
            })?),
            Enum(EnumDefinition { values, .. }) => {
                let ident = String::deserialize(deserializer)?;
                if values.iter().find(|&x| x.value == ident.as_str()).is_none() {
                    return Err(unknown_variant(
                        ident.as_str(),
                        values.iter().map(|vdef| &*vdef.value).collect(),
                    ));
                }
                ConjureValue::Enum(ident)
            }
            Union(union_definition) => ConjureValue::Union(
                deserializer.deserialize_map(ConjureUnionVisitor(&union_definition))?,
            ),
        })
    }
}

impl<'de: 'a, 'a> DeserializeSeed<'de> for &'a EnumDefinition {
    type Value = String;

    fn deserialize<D>(self, de: D) -> Result<Self::Value, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let ident = String::deserialize(de)?;
        if self
            .values
            .iter()
            .find(|&x| x.value == ident.as_str())
            .is_none()
        {
            return Err(unknown_variant(
                ident.as_str(),
                self.values.iter().map(|vdef| &*vdef.value).collect(),
            ));
        }
        Ok(ident)
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
            PrimitiveType::Any => {
                let deser = de.deser()?;
                // We explicitly don't allow 'null' in any.
                if deser == ::serde_value::Value::Option(None) {
                    return Err(::serde::de::Error::custom("unexpected 'null' for type any"));
                }
                ConjurePrimitiveValue::Any(deser)
            }
        };
        Ok(out)
    }
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

/// We don't need DeserializeSeed for Binary because it is a primitive conjure type.
impl<'de> Deserialize<'de> for Binary {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BinaryVisitor;

        impl<'de> Visitor<'de> for BinaryVisitor {
            type Value = Binary;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a base64-encoded string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                let decoded = ::base64::decode(v)
                    .map_err(|e| Error::custom(format_args!("Couldn't decode base64: {}", e)))?;
                Ok(Binary(decoded))
            }
        }

        deserializer.deserialize_str(BinaryVisitor)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use conjure::ir::TypeName;
    use more_serde_json::from_str;

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
            type_name: TypeName {
                name: "Name".to_string(),
                package: "com.palantir.package".to_string(),
            },
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

    #[test]
    fn test_object_collection_fields() {
        let double_type = || ResolvedType::Primitive(PrimitiveType::Double);
        let type_ = ResolvedType::Object(ObjectDefinition {
            type_name: TypeName {
                name: "Name".to_string(),
                package: "com.palantir.package".to_string(),
            },
            fields: vec![
                FieldDefinition {
                    field_name: "foo".to_string(),
                    type_: double_type(),
                },
                FieldDefinition {
                    field_name: "list".to_string(),
                    type_: ResolvedType::List(ListType {
                        item_type: ResolvedType::Primitive(PrimitiveType::Integer).into(),
                    }),
                },
                FieldDefinition {
                    field_name: "set".to_string(),
                    type_: ResolvedType::Set(SetType {
                        item_type: ResolvedType::Primitive(PrimitiveType::Integer).into(),
                    }),
                },
                FieldDefinition {
                    field_name: "map".to_string(),
                    type_: ResolvedType::Map(MapType {
                        key_type: PrimitiveType::String.into(),
                        value_type: ResolvedType::Primitive(PrimitiveType::Integer).into(),
                    }),
                },
            ],
        });

        // Accepts missing collection fields
        assert_eq!(
            from_str(&type_, r#"{"foo": 123}"#).unwrap(),
            ConjureValue::Object(btreemap!(
                "foo" => ConjureValue::Primitive(ConjurePrimitiveValue::double(123.0)),
                "list" => ConjureValue::List(Default::default()),
                "set" => ConjureValue::Set(Default::default()),
                "map" => ConjureValue::Map(Default::default())
            ))
        );

        // Does not tolerate null for collection field
        assert!(from_str(&type_, r#"{list": null, "foo": 123}"#).is_err());

        // Deserializes present collection fields
        let int_value = |v| ConjureValue::Primitive(ConjurePrimitiveValue::Integer(v));
        assert_eq!(
            from_str(&type_, r#"{"list": [1, 2, 3], "foo": 123}"#).unwrap(),
            ConjureValue::Object(btreemap!(
                "foo" => ConjureValue::Primitive(ConjurePrimitiveValue::double(123.0)),
                "list" => ConjureValue::List(vec![1, 2, 3].into_iter().map(int_value).collect()),
                "set" => ConjureValue::Set(Default::default()),
                "map" => ConjureValue::Map(Default::default())
            ))
        );
    }

    #[test]
    fn deser_union() {
        let double_type = || ResolvedType::Primitive(PrimitiveType::Double);
        let type_ = ResolvedType::Union(UnionDefinition {
            type_name: TypeName {
                name: "Name".to_string(),
                package: "com.palantir.package".to_string(),
            },
            union: vec![
                FieldDefinition {
                    field_name: "foo".to_string(),
                    type_: double_type(),
                },
                FieldDefinition {
                    field_name: "bar".to_string(),
                    type_: ResolvedType::Optional(OptionalType {
                        item_type: ResolvedType::Primitive(PrimitiveType::String).into(),
                    }),
                },
            ],
        });

        assert_eq!(
            type_
                .deserialize(&json!({ "type": "foo", "foo": 123 }))
                .unwrap(),
            ConjureValue::Union(ConjureUnionValue {
                field_name: "foo".into(),
                value: ConjureValue::Primitive(ConjurePrimitiveValue::double(123.0)).into(),
            })
        );

        assert_eq!(
            type_
                .deserialize(&json!({ "foo": 123, "type": "foo" }))
                .unwrap(),
            ConjureValue::Union(ConjureUnionValue {
                field_name: "foo".into(),
                value: ConjureValue::Primitive(ConjurePrimitiveValue::double(123.0)).into(),
            })
        );

        assert_eq!(
            type_
                .deserialize(&json!({ "type": "bar", "bar": null }))
                .unwrap(),
            ConjureValue::Union(ConjureUnionValue {
                field_name: "bar".into(),
                value: ConjureValue::Optional(None).into(),
            })
        );

        assert!(type_.deserialize(&json!({ "type": "bar" })).is_err());
        assert!(type_.deserialize(&json!({ "type": "unknown" })).is_err());
        assert!(type_.deserialize(&json!({})).is_err());
        assert!(type_.deserialize(&json!({ "foo": 123 })).is_err());
        assert!(type_.deserialize(&json!({ "bar": 123 })).is_err());
    }
}
