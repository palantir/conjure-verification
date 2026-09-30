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

use syn::punctuated::Punctuated;
use syn::{
    GenericParam, Generics, Path, PredicateType, TraitBound, TraitBoundModifier, Type,
    TypeParamBound, TypePath, WherePredicate,
};

pub fn without_defaults(generics: &Generics) -> Generics {
    Generics {
        params: generics
            .params
            .iter()
            .map(|param| match param {
                GenericParam::Type(param) => {
                    let mut param = param.clone();
                    param.eq_token = None;
                    param.default = None;
                    GenericParam::Type(param)
                }
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
            bounds: Punctuated::from_iter([TypeParamBound::Trait(TraitBound {
                paren_token: None,
                modifier: TraitBoundModifier::None,
                lifetimes: None,
                path: bound.clone(),
            })]),
        })
    });

    let mut generics = generics.clone();
    generics
        .make_where_clause()
        .predicates
        .extend(new_predicates);
    generics
}
