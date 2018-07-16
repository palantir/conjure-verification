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

//! SafeDouble is just a wrapper around f64 but without NaN (because this is not orderable).
//! We also ban Infinity because it can't be represented as a JSON number.

use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;
use std::cmp::Ordering;
use std::error::Error as StdError;
use std::fmt::{self, Display};

/// Represents an `f64` without NAN / INFINITY / NEG_INFINITY.
#[derive(Serialize, Debug, PartialEq, PartialOrd, Display)]
pub struct SafeDouble(f64);

impl SafeDouble {
    pub fn new(v: f64) -> Result<SafeDouble, NotFiniteError> {
        if v.is_finite() {
            Ok(SafeDouble(v))
        } else {
            Err(NotFiniteError(v))
        }
    }
}

impl Eq for SafeDouble {}

impl Ord for SafeDouble {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[derive(Debug)]
pub struct NotFiniteError(f64);

impl StdError for NotFiniteError {}

impl Display for NotFiniteError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!(
            "Only finite numbers are valid SafeDouble. Got: {}",
            self.0
        ))
    }
}

impl<'de> Deserialize<'de> for SafeDouble {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let num = f64::deserialize(deserializer)?;
        SafeDouble::new(num).map_err(|e| Error::custom(e))
    }
}
