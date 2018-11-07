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

//! Data types and logic that resolves [ClientTestCases] into [ResolvedClientTestCases].
//!
//! What is changed between the two is that parsing set of positive tests (lists of strings)
//! becomes a [ResolvedTestCases].
//! This type holds the [resolved conjure type][ResolvedType] for that endpoint, as well as a vec
//! of [ResolvedTestCase] which has the raw test case (string) as well as the parsed [ConjureValue].
//!
//! [ClientTestCases]: ../test_cases/struct.ClientTestCases.html
//! [ResolvedClientTestCases]: ./struct.ResolvedClientTestCases.html
//! [ResolvedTestCases]: struct.ResolvedTestCases.html
//! [ResolvedTestCase]: struct.ResolvedTestCase.html
//! [ResolvedType]: ../../conjure_verification_common/conjure/resolved_type/ResolvedType.t.html
//! [ConjureValue]: ../../conjure_verification_common/conjure/value/struct.ConjureValue.html

use conjure::resolved_type::ResolvedType;
use conjure::value::ConjureValue;
use conjure_verification_common::more_serde_json;
use conjure_verification_error::Error;
use conjure_verification_error::Result;
use errors::VerificationError;
use std::collections::HashMap;
use std::ops::Index;
use test_spec::ClientTestCases;
use test_spec::EndpointName;
use test_spec::PositiveAndNegativeTestCases;

pub struct ResolvedClientTestCases {
    pub auto_deserialize: HashMap<EndpointName, ResolvedPositiveAndNegativeTestCases>,
    pub single_path_param_service: HashMap<EndpointName, ResolvedTestCases>,
    pub single_query_param_service: HashMap<EndpointName, ResolvedTestCases>,
    pub single_header_service: HashMap<EndpointName, ResolvedTestCases>,
}

pub struct ResolvedPositiveAndNegativeTestCases {
    pub positive: ResolvedTestCases,
    pub negative: Vec<String>,
}

pub struct ResolvedTestCases {
    pub conjure_type: ResolvedType,
    pub test_cases: Vec<ResolvedTestCase>,
}

#[derive(Debug)]
pub struct ResolvedTestCase {
    pub value: ConjureValue,
    pub text: String,
}

pub fn resolve_test_cases<'a, I>(
    type_mapping: &'a I,
    client_test_cases: &'a ClientTestCases,
) -> Result<ResolvedClientTestCases>
where
    I: Index<&'a EndpointName, Output = ResolvedType>,
{
    Ok(ResolvedClientTestCases {
        auto_deserialize: client_test_cases
            .auto_deserialize
            .iter()
            .map(|(endpoint, cases)| {
                // Get the conjure type
                let conjure_type = &type_mapping[endpoint];
                // Parse the positive test cases
                let positive = ResolvedTestCases {
                    test_cases: resolve_cases(&cases.positive, &conjure_type, endpoint)
                        .collect::<Result<Vec<_>>>()?,
                    conjure_type: conjure_type.clone(),
                };

                // Ensure none of the negatives can be parsed
                ensure_negative_cases_do_not_parse(&conjure_type, endpoint, cases)?;

                let new_v = ResolvedPositiveAndNegativeTestCases {
                    positive,
                    negative: cases.negative.clone(),
                };
                Ok((endpoint.clone(), new_v))
            }).collect::<Result<HashMap<_, _>>>()?,
        single_path_param_service: client_test_cases
            .single_path_param_service
            .iter()
            .map(|(endpoint, cases)| {
                let conjure_type = &type_mapping[endpoint];
                Ok((
                    endpoint.clone(),
                    ResolvedTestCases {
                        test_cases: resolve_cases(cases, conjure_type, endpoint)
                            .collect::<Result<Vec<_>>>()?,
                        conjure_type: conjure_type.clone(),
                    },
                ))
            }).collect::<Result<HashMap<_, _>>>()?,
        single_query_param_service: client_test_cases
            .single_query_param_service
            .iter()
            .map(|(endpoint, cases)| {
                let conjure_type = &type_mapping[endpoint];
                Ok((
                    endpoint.clone(),
                    ResolvedTestCases {
                        test_cases: resolve_cases(cases, conjure_type, endpoint)
                            .collect::<Result<Vec<_>>>()?,
                        conjure_type: conjure_type.clone(),
                    },
                ))
            }).collect::<Result<HashMap<_, _>>>()?,
        single_header_service: client_test_cases
            .single_header_service
            .iter()
            .map(|(endpoint, cases)| {
                let conjure_type = &type_mapping[endpoint];
                Ok((
                    endpoint.clone(),
                    ResolvedTestCases {
                        test_cases: resolve_cases(cases, conjure_type, endpoint)
                            .collect::<Result<Vec<_>>>()?,
                        conjure_type: conjure_type.clone(),
                    },
                ))
            }).collect::<Result<HashMap<_, _>>>()?,
    })
}

fn ensure_negative_cases_do_not_parse(
    conjure_type: &ResolvedType,
    endpoint: &EndpointName,
    cases: &PositiveAndNegativeTestCases,
) -> Result<()> {
    let parseable_negative: Option<(usize, ResolvedTestCase)> =
        resolve_cases(&cases.negative, conjure_type, endpoint)
            .enumerate()
            .map(|(idx, r)| r.map(|inner| (idx, inner)))
            .filter_map(Result::ok)
            .next();
    if let Some((idx, neg)) = parseable_negative {
        return Err(Error::new_safe(
            "Found negative test case that parsed successfully",
            VerificationError::BadTestCase {
                endpoint: endpoint.clone(),
                index: idx,
                value: neg.text.clone(),
            },
        ));
    }
    return Ok(());
}

fn resolve_cases<'a>(
    cases: &'a [String],
    conjure_type: &'a ResolvedType,
    endpoint: &'a EndpointName,
) -> impl Iterator<Item = Result<ResolvedTestCase>> + 'a {
    cases.iter().enumerate().map(move |(idx, s)| {
        let resolved = deserialize_expected_value(conjure_type, s.as_str(), endpoint, idx);
        resolved.map(|value| ResolvedTestCase {
            text: s.clone(),
            value,
        })
    })
}

fn deserialize_expected_value(
    conjure_type: &ResolvedType,
    raw: &str,
    endpoint: &EndpointName,
    index: usize,
) -> Result<ConjureValue> {
    more_serde_json::from_str(conjure_type, raw).map_err(|e| {
        Error::internal_safe(e)
            .with_safe_param("endpoint", endpoint.to_string())
            .with_safe_param("index", index)
            .with_safe_param("expected_raw", raw)
    })
}
