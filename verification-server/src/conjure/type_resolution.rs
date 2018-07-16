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
use conjure::resolved_type::ResolvedType;

/// Recursively resolve references and aliases to get to the real types.
pub fn resolve_type(types: &Vec<TypeDefinition>, t: &Type) -> ResolvedType {
    match t {
        Type::Reference(name) => {
            // Expect to find this type name in the definitions.
            let definition = types.iter().find(|def| def.type_name() == name).unwrap();
            resolve_type_definition(types, definition)
        }
        Type::Primitive(primitive) => ResolvedType::Primitive(primitive.clone()),
        Type::Optional(inner) => ResolvedType::Optional(OptionalType {
            item_type: resolve_type(types, &inner.item_type).into(),
        }),
        Type::List(inner) => ResolvedType::List(ListType {
            item_type: resolve_type(types, &inner.item_type).into(),
        }),
        Type::Set(inner) => ResolvedType::Set(SetType {
            item_type: resolve_type(types, &inner.item_type).into(),
        }),
        Type::Map(MapType {
            key_type,
            value_type,
        }) => ResolvedType::Map(MapType {
            key_type: (match resolve_type(types, &key_type) {
                ResolvedType::Primitive(prim) => prim,
                it => panic!("Map key type should be primitive but found: {:?}", it),
            }).into(),
            value_type: resolve_type(types, &value_type).into(),
        }),
    }
}

fn resolve_field_definition(
    types: &Vec<TypeDefinition>,
    field_def: &FieldDefinition<Type>,
) -> FieldDefinition<ResolvedType> {
    let &FieldDefinition {
        ref field_name,
        ref type_,
    } = field_def;
    FieldDefinition {
        field_name: field_name.clone(),
        type_: resolve_type(types, type_),
    }
}

fn resolve_type_definition(types: &Vec<TypeDefinition>, t: &TypeDefinition) -> ResolvedType {
    match t {
        TypeDefinition::Alias(alias) => resolve_type(types, &alias.alias),
        TypeDefinition::Enum(enum_) => ResolvedType::Enum(enum_.clone()),
        TypeDefinition::Object(obj) => ResolvedType::Object(ObjectDefinition {
            type_name: obj.type_name.clone(),
            fields: obj.fields
                .iter()
                .map(|defn| resolve_field_definition(types, defn))
                .collect(),
        }),
        TypeDefinition::Union(union) => ResolvedType::Union(UnionDefinition {
            type_name: union.type_name.clone(),
            union: union
                .union
                .iter()
                .map(|defn| resolve_field_definition(types, defn))
                .collect(),
        }),
    }
}
