#[macro_use]
pub extern crate serde;

use serde::de::{Deserialize, Deserializer, Error, IntoDeserializer, Unexpected, Visitor};
use std::fmt;
use std::marker::PhantomData;

/// If the missing field is of type `Option<T>` then treat is as `None`,
/// otherwise it is an error.
pub fn missing_field<'de, V, E>(field: &'static str) -> Result<V, E>
where
    V: Deserialize<'de>,
    E: Error,
{
    struct MissingFieldDeserializer<E>(&'static str, PhantomData<E>);

    impl<'de, E> Deserializer<'de> for MissingFieldDeserializer<E>
    where
        E: Error,
    {
        type Error = E;

        fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, E>
        where
            V: Visitor<'de>,
        {
            Err(Error::missing_field(self.0))
        }

        fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, E>
        where
            V: Visitor<'de>,
        {
            visitor.visit_none()
        }

        forward_to_deserialize_any! {
            bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
            byte_buf unit unit_struct newtype_struct seq tuple tuple_struct map
            struct enum identifier ignored_any
        }
    }

    let deserializer = MissingFieldDeserializer(field, PhantomData);
    Deserialize::deserialize(deserializer)
}

pub enum UnionField<T> {
    Type,
    Data(T),
}

impl<'de, T> Deserialize<'de> for UnionField<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<UnionField<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UnionFieldVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for UnionFieldVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = UnionField<T>;

            fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
                fmt.write_str("field name")
            }

            fn visit_str<E>(self, value: &str) -> Result<UnionField<T>, E>
            where
                E: Error,
            {
                match value {
                    "type" => Ok(UnionField::Type),
                    _ => T::deserialize(value.into_deserializer()).map(UnionField::Data),
                }
            }
        }

        deserializer.deserialize_str(UnionFieldVisitor(PhantomData))
    }
}

pub struct UnionTypeField;

impl<'de> Deserialize<'de> for UnionTypeField {
    fn deserialize<D>(deserializer: D) -> Result<UnionTypeField, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UnionTypeFieldVisitor;

        impl<'de> Visitor<'de> for UnionTypeFieldVisitor {
            type Value = UnionTypeField;

            fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
                fmt.write_str("type field")
            }

            fn visit_str<E>(self, value: &str) -> Result<UnionTypeField, E>
            where
                E: Error,
            {
                match value {
                    "type" => Ok(UnionTypeField),
                    _ => Err(E::invalid_value(Unexpected::Str(value), &self)),
                }
            }
        }

        deserializer.deserialize_str(UnionTypeFieldVisitor)
    }
}
