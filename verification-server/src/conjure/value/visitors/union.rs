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

use conjure::resolved_type::FieldDefinition;
use conjure::resolved_type::UnionDefinition;
use conjure::value::util::unknown_variant;
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
                    Some(UnionField::Data(FieldDefinition { field_name, type_ })) => {
                        if *field_name != variant {
                            return Err(Error::invalid_value(
                                Unexpected::Str(&field_name),
                                &variant.as_ref(),
                            ));
                        }
                        Ok(ConjureUnionValue {
                            field_name: variant,
                            value: items.next_value_seed(type_)?.into(),
                        })
                    }
                    Some(UnionField::Type) | None => {
                        Err(Error::custom(format_args!("missing field `{}`", variant)))
                    }
                }
            }
            Some(UnionField::Data(FieldDefinition { field_name, type_ })) => {
                let value = items.next_value_seed(type_)?.into();
                if let None = items.next_key::<UnionTypeField>()? {
                    return Err(Error::missing_field("type"));
                }
                let variant: String = items.next_value()?;
                if *field_name != variant {
                    return Err(Error::invalid_value(
                        Unexpected::Str(&variant),
                        &field_name.as_ref(),
                    ));
                }
                Ok(ConjureUnionValue {
                    field_name: variant,
                    value,
                })
            }
            None => Err(Error::missing_field("type")),
        }
    }
}

pub enum UnionField<'a> {
    Type,
    Data(&'a FieldDefinition),
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
                match value {
                    "type" => Ok(UnionField::Type),
                    _ => Ok(UnionField::Data(self.0
                        .iter()
                        .find(|fd| fd.field_name == value)
                        .ok_or_else(|| {
                            unknown_variant(
                                value,
                                self.0.iter().map(|fd| fd.field_name.as_str()).collect(),
                            )
                        })?)),
                }
            }
        }

        deserializer.deserialize_str(UnionFieldVisitor(&self.union))
    }
}
