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

use conjure::ir::PrimitiveType;
use conjure::resolved_type::builders::*;
use conjure::resolved_type::FieldDefinition;
use conjure::resolved_type::UnionDefinition;
use conjure::value::*;
use serde::de::DeserializeSeed;
use serde::de::Error;
use serde::de::MapAccess;
use serde::de::Unexpected;
use serde::de::Visitor;
use serde::Deserializer;
use serde_conjure::UnionTypeField;
use std::fmt;

pub struct ConjureUnionVisitor<'a>(pub &'a UnionDefinition);

impl<'de: 'a, 'a> Visitor<'de> for ConjureUnionVisitor<'a> {
    type Value = ConjureUnionValue;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("map")
    }

    fn visit_map<A>(self, mut items: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        match items.next_key_seed(self.0)? {
            Some(UnionField::Type) => {
                let variant: String = items.next_value()?;
                let key = items.next_key_seed(self.0)?;
                match key {
                    Some(UnionField::Data(ref union_variant)) => {
                        fail_if_mismatching_variant(&variant, union_variant)?;
                        build_union_value(&mut items, union_variant)
                    }
                    Some(UnionField::Type) | None => {
                        Err(Error::custom(format_args!("missing field `{}`", variant)))
                    }
                }
            }
            Some(UnionField::Data(ref union_variant)) => {
                let result = build_union_value(&mut items, union_variant);
                if items.next_key::<UnionTypeField>()?.is_none() {
                    return Err(Error::missing_field("type"));
                }
                let variant: String = items.next_value()?;
                fail_if_mismatching_variant(&variant, union_variant)?;
                result
            }
            None => Err(Error::missing_field("type")),
        }
    }
}

pub enum UnionField<'a> {
    Type,
    Data(UnionVariantInner<'a>),
}

pub enum UnionVariantInner<'a> {
    Real(&'a FieldDefinition),
    Unknown(String),
}

impl<'a> UnionVariantInner<'a> {
    pub fn field_name(&self) -> &str {
        match self {
            UnionVariantInner::Real(FieldDefinition { field_name, .. }) => field_name.as_str(),
            UnionVariantInner::Unknown(field_name) => field_name.as_str(),
        }
    }
}

impl<'de: 'a, 'a> DeserializeSeed<'de> for &'a UnionDefinition {
    type Value = UnionField<'a>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UnionFieldVisitor<'a>(&'a Vec<FieldDefinition>);

        impl<'de: 'a, 'a> Visitor<'de> for UnionFieldVisitor<'a> {
            type Value = UnionField<'a>;

            fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
                fmt.write_str("field name")
            }

            fn visit_str<E>(self, value: &str) -> Result<UnionField<'a>, E>
            where
                E: Error,
            {
                Ok(match value {
                    "type" => UnionField::Type,
                    _ => UnionField::Data(
                        self.0
                            .iter()
                            .find(|fd| fd.field_name == value)
                            .map(UnionVariantInner::Real)
                            .unwrap_or_else(|| UnionVariantInner::Unknown(value.to_string())),
                    ),
                })
            }
        }

        deserializer.deserialize_str(UnionFieldVisitor(&self.union))
    }
}

/// The `items` must be in the position to deserialize the union value.
fn build_union_value<'de: 'a, 'a, A>(
    items: &mut A,
    union_variant: &UnionVariantInner,
) -> Result<ConjureUnionValue, A::Error>
where
    A: MapAccess<'de>,
{
    Ok(match union_variant {
        UnionVariantInner::Real(FieldDefinition { type_, field_name }) => ConjureUnionValue {
            variant: UnionVariant::Known(field_name.clone()),
            value: items.next_value_seed(type_)?.into(),
        },
        UnionVariantInner::Unknown(field_name) => ConjureUnionValue {
            variant: UnionVariant::Unknown(field_name.clone()),
            // deserialize it as 'any'
            value: items
                .next_value_seed(&primitive_type(PrimitiveType::Any))?
                .into(),
        },
    })
}

fn fail_if_mismatching_variant<E>(variant: &str, union_variant: &UnionVariantInner) -> Result<(), E>
where
    E: Error,
{
    if union_variant.field_name() != variant {
        return Err(Error::invalid_value(
            Unexpected::Str(union_variant.field_name()),
            &variant.as_ref(),
        ));
    }
    Ok(())
}
