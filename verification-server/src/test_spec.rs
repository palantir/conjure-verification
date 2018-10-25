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

pub use conjure_verification_common::test_spec::EndpointName;
use std::collections::HashMap;

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

#[derive(Deserialize, Debug, Eq, PartialEq, Clone, From)]
pub struct AutoDeserializePositiveTest(pub String);

#[derive(Deserialize, Debug, Eq, PartialEq, Clone, From)]
pub struct AutoDeserializeNegativeTest(pub String);

#[cfg(test)]
mod test {
    use super::*;
    use serde_yaml;
    use std::fs::File;
    use std::path::Path;

    const TEST_CASES_PATH: &str = "../verification-server-api/test-cases.yml";

    #[test]
    fn deserializes_test_cases() {
        let f = File::open(Path::new(TEST_CASES_PATH)).unwrap();
        serde_yaml::from_reader::<_, TestCases>(f).unwrap();
    }
}
