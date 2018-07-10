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
extern crate backtrace;
extern crate itertools;
extern crate regex;
extern crate serde;
extern crate uuid;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_plain;

#[cfg(test)]
extern crate serde_json;

use backtrace::Backtrace;
use itertools::Itertools;
use regex::Regex;
use serde::ser::{Serialize, Serializer};
use std::collections::HashMap;
use std::error;
use std::fmt::Display;
use std::fmt::Formatter;
use std::result;
use uuid::Uuid;

lazy_static! {
    static ref UPPER_CAMEL: Regex = Regex::new("^([A-Z][a-z0-9]+)+$").unwrap();
}

/// A convenience type definition for `Results` with `Error` as the error type.
pub type Result<T> = result::Result<T, Error>;

/// A trait encapsulating the serializable information associated with an Conjure error.
///
/// An application will typically create a single enum implementing this trait with a variant for
/// each error in its API.
///
/// The `conjure-verification-error-derive` crate can be used to automatically implement this trait:
///
/// ```
/// extern crate conjure_verification_error;
/// #[macro_use]
/// extern crate conjure_verification_error_derive;
///
/// #[derive(ErrorType)]
/// // The `#[error_type(namespace = "...")] annotation is require, and determines the error's
/// // namespace.
/// #[error_type(namespace = "MyService")]
/// pub enum MyServiceError {
///     // The `#[error_type(code = "...")] annotation is required, and determines the error's code.
///     // The error's name is determined by the variant's name.
///     //
///     // A unit variant creates an error with no parameters.
///     #[error_type(code = "InvalidArgument")]
///     InvalidColor,
///
///     #[error_type(code = "NotFound")]
///     UnknownResource {
///         // Field names are camel-cased. The `#[error_type(safe)]` annotation identifies
///         // parameters as safe. The types must implement `Display` and `FromStr`.
///         name: String,
///         #[error_type(safe)]
///         resource_id: String,
///     },
/// }
///
/// # fn main() {}
/// ```
pub trait ErrorType {
    /// The namespace of the error.
    ///
    /// This must be upper-camel case.
    const NAMESPACE: &'static str;

    /// Returns the `Code` associated with the error.
    fn code(&self) -> Code;

    /// Returns the name of the error.
    ///
    /// This must be upper-camel case.
    fn name(&self) -> &'static str;

    /// Returns the safe parameters associated with the error.
    fn safe_params(&self) -> HashMap<&'static str, String>;

    /// Returns the unsafe parameters associated with the error.
    ///
    /// # Note
    ///
    /// These unsafe parameters *are* included in the serialized Conjure error.
    fn unsafe_params(&self) -> HashMap<&'static str, String>;

    /// Attempts to parse a serialized Conjure error into an instance of this type.
    fn parse(error: &SerializableError) -> Option<Self>
    where
        Self: Sized;
}

macro_rules! make_code {
    ($($v:ident => $http:expr,)*) => {
        /// A high-level category of an Conjure error.
        ///
        /// `Code` also implements `ErrorType`, and can be used to create generic errors that don't need
        /// more specific information.
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        pub enum Code {
            $($v,)*

            #[doc(hidden)]
            #[serde(skip_serializing, skip_deserializing)]
            __NonExhaustive,
        }

        forward_display_to_serde!(Code);

        impl Code {
            pub fn http_error_code(&self) -> u16 {
                match *self {
                    $(Code::$v => $http,)*
                    Code::__NonExhaustive => unreachable!(),
                }
            }
        }

        impl ErrorType for Code {
            const NAMESPACE: &'static str = "Default";

            fn code(&self) -> Code {
                *self
            }

            fn name(&self) -> &'static str {
                match *self {
                    $(Code::$v => stringify!($v),)*
                    Code::__NonExhaustive => unreachable!(),
                }
            }

            fn safe_params(&self) -> HashMap<&'static str, String> {
                HashMap::new()
            }

            fn unsafe_params(&self) -> HashMap<&'static str, String> {
                HashMap::new()
            }

            fn parse(error: &SerializableError) -> Option<Code> {
                if !error.params().is_empty() {
                    return None;
                }

                match error.name() {
                    $(concat!("Default:", stringify!($v)) => Some(Code::$v),)*
                    _ => None,
                }
            }
        }
    }
}

make_code! {
    PermissionDenied => 403,
    InvalidArgument => 400,
    NotFound => 404,
    Conflict => 409,
    RequestEntityTooLarge => 413,
    FailedPrecondition => 500,
    Internal => 500,
    Timeout => 500,
    CustomClient => 400,
    CustomServer => 500,

    // these don't exist in java http-remoting-api
    TooManyRequests => 429,
    NotAcceptable => 406,
    Unauthorized => 401,
    ServiceUnavailable => 503,
}

#[derive(Debug)]
struct Inner {
    backtraces: Vec<Backtrace>,
    cause: Box<error::Error + Sync + Send>,
    cause_safe: bool,
    code: Code,
    name: String,
    id: Uuid,
    body_params: HashMap<&'static str, String>,
    safe_params: HashMap<&'static str, String>,
    unsafe_params: HashMap<&'static str, String>,
}

/// An Conjure-compatible error type.
///
/// Conjure errors are created from a Rust `Error` cause and an `ErrorType` containing the serializable
/// information for the error. They also contain a randomly generated ID and a backtrace generated
/// at the time of construction.
///
/// This does not implement the `std::error::Error` trait. Applications should convert Rust errors
/// to Conjure errors immediately, and use Conjure errors throughout the application.
#[derive(Debug)]
pub struct Error(Box<Inner>);

impl Error {
    /// Constructs an Conjure error from an unsafe cause.
    pub fn new<E, T>(error: E, error_type: T) -> Error
    where
        E: Into<Box<error::Error + Sync + Send>>,
        T: ErrorType,
    {
        Error::new_inner(error, false, error_type)
    }

    /// Constructs an Conjure error from a safe cause.
    pub fn new_safe<E, T>(error: E, error_type: T) -> Error
    where
        E: Into<Box<error::Error + Sync + Send>>,
        T: ErrorType,
    {
        Error::new_inner(error, true, error_type)
    }

    fn new_inner<E, T>(cause: E, cause_safe: bool, error_type: T) -> Error
    where
        E: Into<Box<error::Error + Sync + Send>>,
        T: ErrorType,
    {
        let inner = Inner {
            backtraces: vec![],
            cause: cause.into(),
            cause_safe,
            code: Code::__NonExhaustive,
            name: String::new(),
            id: Uuid::new_v4(),
            body_params: HashMap::new(),
            safe_params: HashMap::new(),
            unsafe_params: HashMap::new(),
        };

        Error(Box::new(inner))
            .with_type(error_type)
            .with_backtrace()
    }

    /// A convenience function to construct a `Code::Internal` error from an unsafe cause.
    pub fn internal<E>(error: E) -> Error
    where
        E: Into<Box<error::Error + Sync + Send>>,
    {
        Error::new(error, Code::Internal)
    }

    /// A convenience function to construct a `Code::Internal` error from an unsafe cause.
    pub fn internal_safe<E>(error: E) -> Error
    where
        E: Into<Box<error::Error + Sync + Send>>,
    {
        Error::new_safe(error, Code::Internal)
    }

    /// A convenience function to construct a `Code::PermissionDenied` error from an unsafe cause.
    pub fn permission_denied<E>(error: E) -> Error
    where
        E: Into<Box<error::Error + Sync + Send>>,
    {
        Error::new(error, Code::PermissionDenied)
    }

    /// A convenience function to construct a `Code::PermissionDenied` error from a safe cause.
    pub fn permission_denied_safe<E>(error: E) -> Error
    where
        E: Into<Box<error::Error + Sync + Send>>,
    {
        Error::new_safe(error, Code::PermissionDenied)
    }

    /// Replaces the error's `ErrorType` with another.
    ///
    /// Everything except the error's backtraces and cause will be reset.
    pub fn with_type<T>(mut self, error_type: T) -> Error
    where
        T: ErrorType,
    {
        self.type_inner(
            T::NAMESPACE,
            error_type.name(),
            error_type.code(),
            error_type.safe_params(),
            error_type.unsafe_params(),
        );
        self
    }

    fn type_inner(
        &mut self,
        namespace: &'static str,
        name: &'static str,
        code: Code,
        safe_params: HashMap<&'static str, String>,
        unsafe_params: HashMap<&'static str, String>,
    ) {
        assert!(UPPER_CAMEL.is_match(namespace));
        assert!(UPPER_CAMEL.is_match(name));

        self.0.name = format!("{}:{}", namespace, name);
        self.0.code = code;
        self.0.body_params = safe_params
            .iter()
            .chain(unsafe_params.iter())
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        self.0.safe_params = safe_params;
        self.0.unsafe_params = unsafe_params;
    }

    /// Adds a new safe parameter to the error.
    ///
    /// The parameter will be logged, but not included in the serialized representation of the
    /// error.
    pub fn with_safe_param<T>(mut self, key: &'static str, value: T) -> Error
    where
        T: Display,
    {
        self.safe_param_inner(key, value.to_string());
        self
    }

    fn safe_param_inner(&mut self, key: &'static str, value: String) {
        self.0.safe_params.insert(key, value);
    }

    /// Adds a new unsafe parameter to the error.
    ///
    /// The parameter will be logged, but not included in the serialized representation of the
    /// error.
    pub fn with_unsafe_param<T>(mut self, key: &'static str, value: T) -> Error
    where
        T: Display,
    {
        self.unsafe_param_inner(key, value.to_string());
        self
    }

    fn unsafe_param_inner(&mut self, key: &'static str, value: String) {
        self.0.unsafe_params.insert(key, value);
    }

    /// Adds a new backtrace to the error.
    ///
    /// This is intended to use when, for example, transferring the error from one thread to
    /// another.
    pub fn with_backtrace(mut self) -> Error {
        self.0.backtraces.push(Backtrace::new());
        self
    }

    /// Returns the error's ID.
    pub fn id(&self) -> Uuid {
        self.0.id
    }

    /// Returns the error's backtraces, ordered from oldest to newest.
    pub fn backtraces(&self) -> &[Backtrace] {
        &self.0.backtraces
    }

    /// Returns the error's cause.
    pub fn cause(&self) -> &(error::Error + 'static + Sync + Send) {
        &*self.0.cause
    }

    /// Returns whether or not the error's cause is considered safe for logging.
    pub fn cause_safe(&self) -> bool {
        self.0.cause_safe
    }

    /// Returns the error's `Code`.
    pub fn code(&self) -> Code {
        self.0.code
    }

    /// Returns the error's name.
    pub fn name(&self) -> &str {
        &self.0.name
    }

    /// Returns the error's safe parameters.
    pub fn safe_params(&self) -> &HashMap<&'static str, String> {
        &self.0.safe_params
    }

    /// Returns the error's unsafe parameters.
    pub fn unsafe_params(&self) -> &HashMap<&'static str, String> {
        &self.0.unsafe_params
    }

    /// Returns a serializable version of the error.
    pub fn serializable(&self) -> SerializableError {
        SerializableError {
            error_code: self.0.code.to_string(),
            error_name: self.0.name.clone(),
            error_instance_id: self.0.id.to_string(),
            parameters: self.0
                .body_params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
}

// FIXME remove in 0.18
impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.serializable().serialize(serializer)
    }
}

/// A serialized Conjure error.
///
/// This can be deserialized from an Conjure error structure.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializableError {
    error_code: String,
    error_name: String,
    error_instance_id: String,
    parameters: HashMap<String, String>,
}

impl SerializableError {
    pub fn code(&self) -> &str {
        &self.error_code
    }

    pub fn name(&self) -> &str {
        &self.error_name
    }

    pub fn id(&self) -> &str {
        &self.error_instance_id
    }

    pub fn params(&self) -> &HashMap<String, String> {
        &self.parameters
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let params = self.safe_params()
            .iter()
            .chain(self.unsafe_params().iter())
            .map(|(key, value)| format!("{}: {}", key, value))
            .join(", ");
        f.write_fmt(format_args!(
            "{} ({}, id: {}) ({})\nCaused by: {}",
            self.0.name,
            self.0.code,
            self.0.id.to_string(),
            params,
            self.0.cause,
        ))?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn to_serializable() {
        struct TestType;

        impl ErrorType for TestType {
            const NAMESPACE: &'static str = "Test";

            fn code(&self) -> Code {
                Code::CustomClient
            }

            fn name(&self) -> &'static str {
                "Name"
            }

            fn safe_params(&self) -> HashMap<&'static str, String> {
                Some(("foo", "bar".to_string())).into_iter().collect()
            }

            fn unsafe_params(&self) -> HashMap<&'static str, String> {
                Some(("fizz", "buzz".to_string())).into_iter().collect()
            }

            fn parse(_: &SerializableError) -> Option<TestType> {
                unimplemented!()
            }
        }

        let error = Error::new("", TestType);
        let serializable = error.serializable();

        assert_eq!(serializable.code(), "CUSTOM_CLIENT");
        assert_eq!(serializable.name(), "Test:Name");
        assert_eq!(serializable.id(), error.id().to_string());

        let mut params = HashMap::new();
        params.insert("foo".to_string(), "bar".to_string());
        params.insert("fizz".to_string(), "buzz".to_string());
        assert_eq!(serializable.params(), &params);
    }

    #[test]
    fn round_trip() {
        let error = r#"
            {
                "errorCode": "INTERNAL",
                "errorName": "Default:Internal",
                "errorInstanceId": "1234",
                "parameters": {
                    "foo": "bar"
                }
            }
        "#;
        let error = serde_json::from_str::<SerializableError>(error).unwrap();
        let json = serde_json::to_string(&error).unwrap();
        let error2 = serde_json::from_str::<SerializableError>(&json).unwrap();

        assert_eq!(error, error2);
    }

    #[test]
    fn other_json_doesnt_deserialize() {
        let error = r#"
            {
                "myCustomErrorMessage": "hello"
            }
        "#;
        assert!(serde_json::from_str::<SerializableError>(error).is_err());
    }
}
