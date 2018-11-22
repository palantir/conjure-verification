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

use conjure_verification_common::conjure::ir::Conjure;
use serde_json;
use std::fs::File;
use std::path::Path;
use test_spec::TestCases;

const TEST_CASES_PATH: &str = "../verification-server-api/build/test-cases.json";
const CONJURE_IR_PATH: &str =
    "../verification-server-api/build/conjure-ir/verification-server-api.conjure.json";

/// This test requires that you run `./gradlew compileIr compileTestCasesJson` beforehand.
#[test]
fn can_parse_all_test_cases() {
    let test_cases_file = File::open(Path::new(TEST_CASES_PATH)).unwrap();
    let test_cases: TestCases = serde_json::from_reader(test_cases_file).unwrap();

    let ir_file = File::open(Path::new(CONJURE_IR_PATH)).unwrap();
    let ir: Conjure = serde_json::from_reader(ir_file).unwrap();

    ::resolve_test_cases(&ir, &test_cases.client).unwrap();
}
