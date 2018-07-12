use ir::*;

pub enum ResolvedType {
    Primitive(PrimitiveType),
    // declared types
    Object(ObjectDefinition<ResolvedType>),
    Enum(EnumDefinition),
    Union(UnionDefinition<ResolvedType>),
    // anonymous wrapper types
    Optional(OptionalType<ResolvedType>),
    List(ListType<ResolvedType>),
    Set(SetType<ResolvedType>),
    Map(MapType<ResolvedType, ResolvedType>),
}

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
            key_type: resolve_type(types, &key_type).into(),
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
        TypeDefinition::Alias(alias) => resolve_type(types, &alias.definition.alias),
        TypeDefinition::Enum(enum_) => ResolvedType::Enum(enum_.definition.clone()),
        TypeDefinition::Object(obj) => ResolvedType::Object(ObjectDefinition {
            fields: obj.definition
                .fields
                .iter()
                .map(|defn| resolve_field_definition(types, defn))
                .collect(),
        }),
        TypeDefinition::Union(union) => ResolvedType::Union(UnionDefinition {
            union: union
                .definition
                .union
                .iter()
                .map(|defn| resolve_field_definition(types, defn))
                .collect(),
        }),
    }
}
