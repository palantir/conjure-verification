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
extern crate conjure_verification_error;

#[macro_use]
extern crate conjure_verification_error_derive;

use conjure_verification_error::{Code, Error, ErrorType};
use std::collections::HashMap;
use std::fmt::Debug;

fn round_trip<T>(error: T)
where
    T: Clone + ErrorType + PartialEq + Debug,
{
    let e = Error::new("", error.clone());
    let s = e.serializable();
    assert_eq!(T::parse(&s), Some(error), "{:?}", s);
}

#[test]
fn basic() {
    #[derive(ErrorType, PartialEq, Debug, Clone)]
    #[error_type(namespace = "Foobar")]
    enum FoobarError {
        #[error_type(code = "InvalidArgument")]
        BadFuzz,
        #[error_type(code = "NotFound")]
        InvalidFizz {
            the_count: u32,
            #[error_type(safe)]
            expected: String,
        },
    }

    assert_eq!(FoobarError::NAMESPACE, "Foobar");
    let e = FoobarError::BadFuzz;
    assert_eq!(e.code(), Code::InvalidArgument);
    assert_eq!(e.name(), "BadFuzz");
    assert_eq!(e.safe_params(), HashMap::new());
    assert_eq!(e.unsafe_params(), HashMap::new());
    round_trip(e);

    let e = FoobarError::InvalidFizz {
        the_count: 10,
        expected: "foobar".to_string(),
    };
    assert_eq!(e.code(), Code::NotFound);
    assert_eq!(e.name(), "InvalidFizz");
    let mut safe_params = HashMap::new();
    safe_params.insert("expected", "foobar".to_string());
    assert_eq!(e.safe_params(), safe_params);
    let mut unsafe_params = HashMap::new();
    unsafe_params.insert("theCount", "10".to_string());
    assert_eq!(e.unsafe_params(), unsafe_params);
    round_trip(e);
}
