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
use conjure_verification_error::Error;
use conjure_verification_error::Result;
use core::result::Result as StdResult;
use either::{Either, Left, Right};
use errors::VerificationError;
use serde_json;
use serde_value::Value;
use serde_yaml;
use std::collections::HashMap;
use std::io;
use std::str::FromStr;

#[derive(Deserialize, Debug, Eq, PartialEq, Hash, Clone, From, Display)]
pub struct EndpointName(pub String);

impl FromStr for EndpointName {
    type Err = ();

    fn from_str(s: &str) -> StdResult<Self, <Self as FromStr>::Err> {
        Ok(EndpointName::new(s))
    }
}

impl EndpointName {
    pub fn new(string: &str) -> EndpointName {
        EndpointName(string.into())
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TestCases {
    pub client: ClientTestCases,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientTestCases {
    pub auto_deserialize: HashMap<EndpointName, PositiveAndNegativeTestCases>,
    pub single_path_param_service: HashMap<EndpointName, Vec<String>>,
    pub single_query_param_service: HashMap<EndpointName, Vec<String>>,
    pub single_header_service: HashMap<EndpointName, Vec<String>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PositiveAndNegativeTestCases {
    pub positive: Vec<String>,
    pub negative: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTest {
    endpoint_name: String,
    auth: Option<String>,
    #[serde(default)]
    header_params: HashMap<String, String>,
    #[serde(default)]
    path_params: HashMap<String, Value>,
    request: Option<ExpectedRequest>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedRequest {
    type_name: String,
    value: Value,
}

/// The full index among `PositiveAndNegativeTests` where positives start at index 0, and after them
/// come the negative tests.
#[derive(Debug, Eq, Ord, PartialOrd, PartialEq, From, Hash, Display)]
pub struct TestIndex(usize);

#[derive(Deserialize, Debug, Eq, PartialEq, Clone, From)]
pub struct AutoDeserializePositiveTest(pub String);

#[derive(Deserialize, Debug, Eq, PartialEq, Clone, From)]
pub struct AutoDeserializeNegativeTest(pub String);

impl PositiveAndNegativeTestCases {
    pub fn index(
        &self,
        index: &TestIndex,
    ) -> Result<Either<AutoDeserializePositiveTest, AutoDeserializeNegativeTest>> {
        let positives = self.positive.len();
        let negatives = self.negative.len();
        let index_out_of_bounds = || {
            Error::new_safe(
                "Index out of bounds",
                VerificationError::IndexOutOfBounds {
                    index: index.0,
                    max_index: positives + negatives,
                },
            )
        };
        let is_negative_test = index.0 >= positives;
        let result = if is_negative_test {
            let test = self
                .negative
                .get(index.0 - positives)
                .ok_or_else(index_out_of_bounds)?;
            Right(test.clone().into())
        } else {
            Left(self.positive[index.0].clone().into())
        };
        Ok(result)
    }
}

#[allow(dead_code)]
pub fn from_yaml_file<R>(rdr: R) -> serde_yaml::Result<TestCases>
where
    R: io::Read,
{
    serde_yaml::from_reader(rdr)
}

pub fn from_json_file<R>(rdr: R) -> serde_json::Result<TestCases>
where
    R: io::Read,
{
    serde_json::from_reader(rdr)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;
    use std::path::Path;

    const TEST_CASES_PATH: &str = "../test-cases.yml";

    #[test]
    fn deserializes_test_cases() {
        let f = File::open(Path::new(TEST_CASES_PATH)).unwrap();
        from_yaml_file(f).unwrap();
    }
}
