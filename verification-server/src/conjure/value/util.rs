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

use itertools::Itertools;
use serde::de::Error;

/// Shameless kinda copied from serde::de::Error::unknown_variant because they only take static strings.
pub fn unknown_variant<'a, E: Error>(field: &'a str, expected: Vec<&'a str>) -> E {
    if expected.is_empty() {
        Error::custom(format_args!(
            "unknown variant `{}`, there are no variants",
            field
        ))
    } else {
        Error::custom(format_args!(
            "unknown variant `{}`, expected one of: {}",
            field,
            expected.into_iter().join(", ")
        ))
    }
}
