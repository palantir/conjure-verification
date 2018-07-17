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

use conjure::resolved_type::ResolvedType;
use conjure::value::*;
use core::fmt;
use serde::de::Error;
use serde::de::SeqAccess;
use serde::de::Visitor;
use serde::Deserializer;
use serde_json;
use std::collections::BTreeSet;

pub struct ConjureSetVisitor<'a> {
    pub item_type: &'a ResolvedType,
    pub fail_on_duplicates: bool,
}

impl<'de: 'a, 'a> Visitor<'de> for ConjureSetVisitor<'a> {
    type Value = BTreeSet<ConjureValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a sequence")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = BTreeSet::new();

        while let Some(value) = seq.next_element_seed(self.item_type)? {
            if self.fail_on_duplicates && values.contains(&value) {
                return Err(Error::custom(format_args!(
                    "Set contained duplicates: {}",
                    serde_json::ser::to_string(&value).unwrap()
                )));
            }
            values.insert(value);
        }

        Ok(values)
    }
}
