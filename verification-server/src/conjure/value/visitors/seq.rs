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

use conjure::resolved_type::ResolvedType;
use conjure::value::*;
use serde::de::SeqAccess;
use serde::de::Visitor;
use serde::private::de::size_hint;
use std::fmt;

pub struct ConjureSeqVisitor<'a>(pub &'a ResolvedType);

impl<'de: 'a, 'a> Visitor<'de> for ConjureSeqVisitor<'a> {
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
