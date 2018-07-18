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

//! ConjureDouble is a wrapper around f64 that assigns deterministic Eq and Ord implementations to
//! NaN and +/- INFINITY.

use serde::de::Error;
use serde::de::IntoDeserializer;
use serde::de::Visitor;
use serde::Deserialize;
use serde::Deserializer;
use std::cmp::Ordering;
use std::fmt::{self, Display};
use std::num::ParseFloatError;
use std::str::FromStr;

/// Represents a finite `f64` or NaN / NegativeInfinity / PositiveInfinity.
#[derive(Serialize, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum ConjureDouble {
    NaN,
    NegativeInfinity,
    Finite(FiniteDouble),
    PositiveInfinity,
}

/// Represents a finite `f64` (which cannot be NaN / NegativeInfinity / PositiveInfinity).
/// Field is private so users can't create a FiniteDouble that's not actually finite.
/// To access the value, use `FiniteDouble::value`.
#[derive(Serialize, Debug, PartialEq, PartialOrd, Display)]
pub struct FiniteDouble(f64);

impl FiniteDouble {
    #[allow(dead_code)]
    fn value(&self) -> f64 {
        self.0
    }
}

impl ConjureDouble {
    pub fn new(v: f64) -> ConjureDouble {
        match v {
            v if v.is_nan() => ConjureDouble::NaN,
            v if v.is_finite() => ConjureDouble::Finite(FiniteDouble(v)),
            v if v.is_infinite() => {
                if v.is_sign_positive() {
                    ConjureDouble::PositiveInfinity
                } else {
                    ConjureDouble::NegativeInfinity
                }
            }
            _ => {
                panic!("Cannot interpret f64 as ConjureDouble: {}", v);
            }
        }
    }
}

impl Display for ConjureDouble {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ConjureDouble::NaN => f.write_str("NaN"),
            ConjureDouble::NegativeInfinity => f.write_str("NegativeInfinity"),
            ConjureDouble::PositiveInfinity => f.write_str("PositiveInfinity"),
            ConjureDouble::Finite(ref fd) => f.write_fmt(format_args!("{}", fd.0)),
        }
    }
}

impl Eq for FiniteDouble {}

impl Ord for FiniteDouble {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

// Deserialization

impl<'de> Deserialize<'de> for ConjureDouble {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        enum LiteralValue {
            NaN,
            PositiveInfinity,
            NegativeInfinity,
        }

        struct ConjureDoubleVisitor;

        impl<'de> Visitor<'de> for ConjureDoubleVisitor {
            type Value = ConjureDouble;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "a float or an string value of NaN, NegativeInfinity, \
                     or PositiveInfinity",
                )
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                self.visit_f64(v as f64)
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                self.visit_f64(v as f64)
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                // Relying on ::new to ensure that if this value passed in by the deserializer is somehow
                // not finite, it ends up being deserialized to the correct variant.
                Ok(ConjureDouble::new(v))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                let literal = LiteralValue::deserialize(v.into_deserializer())?;
                Ok(match literal {
                    LiteralValue::NaN => ConjureDouble::NaN,
                    LiteralValue::NegativeInfinity => ConjureDouble::NegativeInfinity,
                    LiteralValue::PositiveInfinity => ConjureDouble::PositiveInfinity,
                })
            }
        }

        deserializer.deserialize_any(ConjureDoubleVisitor)
    }
}

/// Deserialization from string, because some deserializers (serde_plain) don't support
/// deserialize_any.
impl FromStr for ConjureDouble {
    type Err = ParseFloatError;

    fn from_str(s: &str) -> Result<Self, <Self as FromStr>::Err> {
        Ok(match s {
            "NaN" => ConjureDouble::NaN,
            "PositiveInfinity" => ConjureDouble::PositiveInfinity,
            "NegativeInfinity" => ConjureDouble::NegativeInfinity,
            _ => ConjureDouble::new(s.parse()?),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn deser_json() {
        let des: ConjureDouble = ::serde_json::from_str(r#""NaN""#).unwrap();
        assert_eq!(des, ConjureDouble::NaN);

        let des: ConjureDouble = ::serde_json::from_str(r#""PositiveInfinity""#).unwrap();
        assert_eq!(des, ConjureDouble::PositiveInfinity);

        let des: ConjureDouble = ::serde_json::from_str(r#""NegativeInfinity""#).unwrap();
        assert_eq!(des, ConjureDouble::NegativeInfinity);

        let des: ConjureDouble = ::serde_json::from_str("-0").unwrap();
        assert_eq!(des, ConjureDouble::Finite(FiniteDouble(-0.0)));
    }

    #[test]
    fn deser_from_str() {
        let des: ConjureDouble = "NaN".parse().unwrap();
        assert_eq!(des, ConjureDouble::NaN);

        let des: ConjureDouble = "PositiveInfinity".parse().unwrap();
        assert_eq!(des, ConjureDouble::PositiveInfinity);

        let des: ConjureDouble = "NegativeInfinity".parse().unwrap();
        assert_eq!(des, ConjureDouble::NegativeInfinity);

        let des: ConjureDouble = "-0".parse().unwrap();
        assert_eq!(des, ConjureDouble::Finite(FiniteDouble(-0.0)));
    }
}
