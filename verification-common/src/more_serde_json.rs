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

//! Two helpers adapted from serde_json to allow us to do contextual deserialization into our
//! conjure::value::ConjureValue type, where serde_json only provides helpers to do
//! context-free deserializing.
//!
//! This could in theory be contributed upstream.

use serde::de::DeserializeSeed;
use serde_json;

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
