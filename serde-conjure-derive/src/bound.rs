use syn::{
    GenericParam, Generics, Path, PredicateType, TraitBound, TraitBoundModifier, Type, TypeParam,
    TypeParamBound, TypePath, WherePredicate,
};

pub fn without_defaults(generics: &Generics) -> Generics {
    Generics {
        params: generics
            .params
            .iter()
            .map(|param| match param {
                GenericParam::Type(param) => GenericParam::Type(TypeParam {
                    eq_token: None,
                    default: None,
                    ..param.clone()
                }),
                _ => param.clone(),
            })
            .collect(),
        ..generics.clone()
    }
}

pub fn with_bound(generics: &Generics, bound: &Path) -> Generics {
    let new_predicates = generics.type_params().map(|ty| {
        WherePredicate::Type(PredicateType {
            lifetimes: None,
            bounded_ty: Type::Path(TypePath {
                qself: None,
                path: ty.ident.clone().into(),
            }),
            colon_token: Default::default(),
            bounds: Some(TypeParamBound::Trait(TraitBound {
                paren_token: None,
                modifier: TraitBoundModifier::None,
                lifetimes: None,
                path: bound.clone(),
            })).into_iter()
                .collect(),
        })
    });

    let mut generics = generics.clone();
    generics
        .make_where_clause()
        .predicates
        .extend(new_predicates);
    generics
}
