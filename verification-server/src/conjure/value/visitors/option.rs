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
//!     conjure::resolved_type::ResolvedType into a conjure::value::ConjureValue
//!
//! Note, there is no internal mutability here, we just use DeserializeSeed to pass information into
//! the deserialization process (contextual deserialization) instead of the usual context-free
//! deserialization.

use conjure::resolved_type::ResolvedType;
use conjure::value::*;
use core::fmt;
pub use serde::de::DeserializeSeed;
use serde::de::Visitor;
use serde::Deserializer;
use std::error::Error as StdError;

pub struct ConjureOptionVisitor<'a>(pub &'a ResolvedType);

impl<'de: 'a, 'a> Visitor<'de> for ConjureOptionVisitor<'a> {
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
