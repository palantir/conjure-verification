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

use conjure::ir::*;

/// Similar to the conjure::ir::Type, but doesn't have a `Reference` variant.
/// Instead, these are inlined.
#[derive(Debug)]
pub enum ResolvedType {
    // named types
    Object(ObjectDefinition<ResolvedType>),
    Enum(EnumDefinition),
    Union(UnionDefinition<ResolvedType>),

    // anonymous types
    Primitive(PrimitiveType),
    Optional(OptionalType<ResolvedType>),
    List(ListType<ResolvedType>),
    Set(SetType<ResolvedType>),
    Map(MapType<PrimitiveType, ResolvedType>),
}
