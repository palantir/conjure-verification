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
extern crate bytes;
extern crate conjure_verification_error;
extern crate http;
extern crate mime;
extern crate serde;
extern crate serde_json;
extern crate typed_headers;

#[macro_use]
extern crate conjure_verification_error_derive;

use mime::{Mime, APPLICATION_JSON};
use request::Format;

pub mod auth;
pub mod error;
pub mod request;
pub mod resource;
pub mod response;

#[derive(Copy, Clone)]
pub enum SerializableFormat {
    Json,
}

impl Format for SerializableFormat {
    fn mime(&self) -> &Mime {
        match *self {
            SerializableFormat::Json => &APPLICATION_JSON,
        }
    }
}
