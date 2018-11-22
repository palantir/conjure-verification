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

#[derive(ConjureDeserialize, Debug)]
pub struct TestCases {
    pub client: ClientTestCases,
}

#[derive(ConjureDeserialize, Debug, Default)]
pub struct ClientTestCases {
    pub auto_deserialize: HashMap<EndpointName, PositiveAndNegativeTestCases>,
    pub single_path_param_service: HashMap<EndpointName, Vec<String>>,
    pub single_query_param_service: HashMap<EndpointName, Vec<String>>,
    pub single_header_service: HashMap<EndpointName, Vec<String>>,
}

#[derive(ConjureDeserialize, Debug)]
pub struct PositiveAndNegativeTestCases {
    pub positive: Vec<String>,
    pub negative: Vec<String>,
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json;
    use std::fs::File;
    use std::path::Path;

    const TEST_CASES_JSON: &str = "../verification-server-api/build/test-cases.json";

    #[test]
    fn deserializes_test_cases_json() {
        let path = Path::new(TEST_CASES_JSON);
        assert_eq!(
            path.exists(),
            true,
            "file missing, run ./gradlew compileTestCasesJson to generate it"
        );

        let f = File::open(path).unwrap();
        let test_cases: TestCases = serde_json::from_reader(f).unwrap();

        eprintln!(
            "Deserialized {} testcases",
            count_test_cases(&test_cases.client)
        );
    }

    fn count_test_cases(test_cases: &ClientTestCases) -> usize {
        let auto_deserialize: usize = test_cases
            .auto_deserialize
            .iter()
            .map(|(_, v)| v.negative.len() + v.positive.len())
            .sum();

        let count = |map: &HashMap<_, Vec<_>>| map.iter().map(|(_, v)| v.len()).sum::<usize>();

        auto_deserialize
            + count(&test_cases.single_header_service)
            + count(&test_cases.single_path_param_service)
            + count(&test_cases.single_query_param_service)
    }
}
