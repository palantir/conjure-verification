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

//! Defines deserialization for the [PLAIN format]
//!
//! [PLAIN format]: https://github.com/palantir/conjure/blob/develop/docs/spec/wire.md#plain-format

use conjure::ir::PrimitiveType;
use conjure::resolved_type::ResolvedType;
use conjure::value::double::ConjureDouble;
use conjure::value::ConjurePrimitiveValue;
use conjure::value::ConjureValue;
use serde::de::DeserializeSeed;
use serde_plain;

/// Deserializes the string using the PLAIN format for the given conjure type.
///
/// Error is boxed as it can be multiple different error types.
pub fn deserialize_plain(
    conjure_type: &ResolvedType,
    str: &str,
) -> Result<ConjureValue, Box<::std::error::Error + Send + Sync>> {
    match *conjure_type {
        ResolvedType::Primitive(ref primitive_type) => Ok(ConjureValue::Primitive(
            deserialize_plain_primitive(primitive_type, str)?,
        )),
        ResolvedType::Enum(ref enum_def) => {
            let de = serde_plain::Deserializer::from_str(&str);
            Ok(ConjureValue::Enum(enum_def.deserialize(de)?))
        }
        _ => Err(format!("Unsupported conjure type: {:?}", conjure_type).into()),
    }
}

/// Deserializes the string using the PLAIN format for the given conjure [PrimitiveType].
///
/// Error is boxed as it can be multiple different error types.
///
/// [PrimitiveType]: ../../ir/enum.PrimitiveType.html
pub fn deserialize_plain_primitive(
    conjure_type: &PrimitiveType,
    str: &str,
) -> Result<ConjurePrimitiveValue, Box<::std::error::Error + Send + Sync>> {
    // Hack: serde_plain can't accept deserialize_any which is what ConjureDouble's
    // deserializer uses, so we special case that type, knowing that this case only
    // supports primitive types anyway.
    if let PrimitiveType::Double = conjure_type {
        Ok(ConjurePrimitiveValue::Double(str.parse::<ConjureDouble>()?))
    } else {
        let de = serde_plain::Deserializer::from_str(&str);
        conjure_type.deserialize(de).map_err(From::from)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use conjure::resolved_type::builders::*;

    #[test]
    fn test_deserialize_enum() {
        let enum_def  = enum_definition("whatev", &["foo", "bar"]);
        deserialize_plain(&enum_def, "foo").unwrap();
        deserialize_plain(&enum_def, "bar").unwrap();
        deserialize_plain(&enum_def, "baz").expect_err("Should fail");
    }
}
