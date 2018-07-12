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
