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
use serde_json;
use serde_value::Value;
//use either::{Either, Left, Right};
use chrono::DateTime;
use chrono::FixedOffset;
use conjure_verification_error::{Code, Error};
use core::fmt;
use serde::de::DeserializeSeed;
use serde::de::Visitor;
use std::any::Any;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error;
use std::error::Error as StdError;
use std::marker::PhantomData;
use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub enum ConjurePrimitiveValue {
    String(String),
    Integer(i32),
    Double(f64),
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

#[derive(Debug, PartialEq)]
pub enum ConjureValue {
    Primitive(ConjurePrimitiveValue),
    // complex
    Optional(Option<Box<ConjureValue>>),
    Object(BTreeMap<String, ConjureValue>),
    Enum(String),
    Union {
        field_name: String,
        value: Box<ConjureValue>,
    },
    // anonymous
    List(Vec<ConjureValue>),
    Set(BTreeSet<ConjureValue>),
    Map(BTreeMap<ConjurePrimitiveValue, ConjureValue>),
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

impl<'de: 'a, 'a> DeserializeSeed<'de> for &'a ResolvedType {
    type Value = ConjureValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let deser_primitive =
            |de: D, pt: &PrimitiveType| -> Result<ConjurePrimitiveValue, D::Error> {
                let out = match *pt {
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
            };

        Ok(match self {
            Primitive(p) => ConjureValue::Primitive(deser_primitive(deserializer, p)?),
            Optional(OptionalType { item_type }) => ConjureValue::Optional(
                deserializer
                    .deserialize_option(OptionVisitor(&item_type))?
                    .map(Box::new),
            ),
            _ => unimplemented!(),
        })
    }
}

pub fn from_str<'de: 'a, 'a, T>(seed: T, str: &'de str) -> serde_json::Result<T::Value>
where
    T: DeserializeSeed<'de>,
{
    from_trait(seed, serde_json::de::StrRead::new(str))
}

pub fn from_trait<'de: 'a, 'a, R, T>(seed: T, read: R) -> serde_json::Result<T::Value>
where
    R: serde_json::de::Read<'de>,
    T: DeserializeSeed<'de>,
{
    let mut de = serde_json::Deserializer::new(read);
    let value = seed.deserialize(&mut de)?;

    // Make sure the whole stream has been consumed.
    de.end()?;
    Ok(value)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_double() {
        let type_ = ResolvedType::Primitive(PrimitiveType::Double);
        assert_eq!(
            from_str(&type_, "123").unwrap(),
            ConjureValue::Primitive(ConjurePrimitiveValue::Double(123.0))
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
                ConjureValue::Primitive(ConjurePrimitiveValue::Double(123.0)).into()
            ))
        );
        assert_eq!(
            from_str(&type_, "null").unwrap(),
            ConjureValue::Optional(None)
        );
    }
}
