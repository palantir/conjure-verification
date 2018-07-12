extern crate serde_json;

#[macro_use]
extern crate serde_conjure_derive;

use serde_json::from_str;

#[test]
fn basic_struct() {
    #[derive(ConjureDeserialize, PartialEq, Debug)]
    struct Foo {
        type_: String,
        fizz_buzz: i32,
    }

    assert_eq!(
        from_str::<Foo>(
            r#"{
  "type": "thingy",
  "fizzBuzz": 15
}"#,
        ).unwrap(),
        Foo {
            type_: "thingy".to_string(),
            fizz_buzz: 15,
        }
    );

    assert_eq!(
        from_str::<Foo>(
            r#"{
  "fizzBuzz": 15,
  "type": "thingy"
}"#,
        ).unwrap(),
        Foo {
            type_: "thingy".to_string(),
            fizz_buzz: 15,
        }
    );
}

#[test]
fn optional_field() {
    #[derive(ConjureDeserialize, PartialEq, Debug)]
    struct Foo {
        type_: Option<String>,
        fizz_buzz: i32,
    }

    assert_eq!(
        from_str::<Foo>(
            r#"{
  "type": "thingy",
  "fizzBuzz": 15
}"#,
        ).unwrap(),
        Foo {
            type_: Some("thingy".to_string()),
            fizz_buzz: 15,
        }
    );

    assert_eq!(
        from_str::<Foo>(
            r#"{
  "fizzBuzz": 15
}"#,
        ).unwrap(),
        Foo {
            type_: None,
            fizz_buzz: 15,
        }
    );

    assert_eq!(
        from_str::<Foo>(
            r#"{
  "type": null,
  "fizzBuzz": 15
}"#,
        ).unwrap(),
        Foo {
            type_: None,
            fizz_buzz: 15,
        }
    );
}

#[test]
fn generic_struct() {
    #[derive(ConjureDeserialize, PartialEq, Debug)]
    struct Foo<T> {
        type_: T,
        fizz_buzz: i32,
    }

    assert_eq!(
        from_str::<Foo<String>>(
            r#"{
  "type": "thingy",
  "fizzBuzz": 15
}"#,
        ).unwrap(),
        Foo {
            type_: "thingy".to_string(),
            fizz_buzz: 15,
        }
    );
}

#[test]
fn c_like() {
    #[derive(ConjureDeserialize, PartialEq, Debug)]
    enum Foo {
        Bar,
        FizzBuzz,
    }

    assert_eq!(from_str::<Foo>("\"BAR\"").unwrap(), Foo::Bar);
    assert_eq!(from_str::<Foo>("\"FIZZ_BUZZ\"").unwrap(), Foo::FizzBuzz);
}

#[test]
fn union() {
    #[derive(ConjureDeserialize, PartialEq, Debug)]
    enum Foo {
        Bar(String),
        FizzBuzz(FizzBuzz),
    }

    #[derive(ConjureDeserialize, PartialEq, Debug)]
    struct FizzBuzz {
        foo: u32,
    }

    assert_eq!(
        from_str::<Foo>(
            r#"{
  "type": "bar",
  "bar": "hello"
}"#,
        ).unwrap(),
        Foo::Bar("hello".to_string())
    );

    assert_eq!(
        from_str::<Foo>(
            r#"{
  "bar": "hello",
  "type": "bar"
}"#,
        ).unwrap(),
        Foo::Bar("hello".to_string())
    );

    assert_eq!(
        from_str::<Foo>(
            r#"{
  "type": "fizzBuzz",
  "fizzBuzz": {
    "foo": 15
  }
}"#,
        ).unwrap(),
        Foo::FizzBuzz(FizzBuzz { foo: 15 })
    );
}

#[test]
fn single_variant_union() {
    #[derive(ConjureDeserialize, PartialEq, Debug)]
    enum Foo {
        Bar(String),
    }

    assert_eq!(
        from_str::<Foo>(
            r#"{
  "type": "bar",
  "bar": "hello"
}"#,
        ).unwrap(),
        Foo::Bar("hello".to_string())
    );
}
