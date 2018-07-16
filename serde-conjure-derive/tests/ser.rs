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

extern crate serde_json;

#[macro_use]
extern crate serde_conjure_derive;

use serde_json::to_string_pretty;

#[test]
fn basic() {
    #[derive(ConjureSerialize)]
    struct Foo {
        type_: String,
        fizz_buzz: i32,
    }

    let foo = Foo {
        type_: "thingy".to_string(),
        fizz_buzz: 15,
    };
    let s = to_string_pretty(&foo).unwrap();
    assert_eq!(
        s,
        r#"{
  "type": "thingy",
  "fizzBuzz": 15
}"#
    );
}

#[test]
fn generic() {
    #[derive(ConjureSerialize)]
    struct Foo<T>
    where
        T: Copy,
    {
        foo: T,
    }

    let foo = Foo { foo: 15 };
    let s = to_string_pretty(&foo).unwrap();
    assert_eq!(
        s,
        r#"{
  "foo": 15
}"#
    );
}

#[test]
fn c_like() {
    #[derive(ConjureSerialize)]
    enum Foo {
        Bar,
        FizzBuzz,
    }

    assert_eq!(to_string_pretty(&Foo::Bar).unwrap(), "\"BAR\"");
    assert_eq!(to_string_pretty(&Foo::FizzBuzz).unwrap(), "\"FIZZ_BUZZ\"");
}

#[test]
fn union() {
    #[derive(ConjureSerialize)]
    enum Foo {
        Bar(String),
        FizzBuzz(FizzBuzz),
    }

    #[derive(ConjureSerialize)]
    struct FizzBuzz {
        foo: u32,
    }

    let foo = Foo::Bar("hello".to_string());
    assert_eq!(
        to_string_pretty(&foo).unwrap(),
        r#"{
  "type": "bar",
  "bar": "hello"
}"#
    );

    let foo = Foo::FizzBuzz(FizzBuzz { foo: 15 });
    assert_eq!(
        to_string_pretty(&foo).unwrap(),
        r#"{
  "type": "fizzBuzz",
  "fizzBuzz": {
    "foo": 15
  }
}"#
    );
}

#[test]
fn generic_union() {
    #[derive(ConjureSerialize)]
    enum Foo<T> {
        Bar(T),
    }

    let foo = Foo::Bar(1);
    assert_eq!(
        to_string_pretty(&foo).unwrap(),
        r#"{
  "type": "bar",
  "bar": 1
}"#
    );
}
