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

use proc_macro2::{Span, TokenStream};
use syn::{self, Data, DataEnum, DataStruct, DeriveInput, Fields, FieldsNamed, Ident};

use bound;
use {pascal_to_camel, pascal_to_screaming, snake_to_camel, EnumKind};

pub fn expand_derive_serialize(input: &DeriveInput) -> Result<TokenStream, String> {
    let ident = &input.ident;

    let generics = bound::without_defaults(&input.generics);
    let generics = bound::with_bound(&generics, &parse_quote!(_serde_conjure::serde::Serialize));
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let dummy_const = Ident::new(&format!("_IMPL_SERIALIZE_FOR_{}", ident), Span::call_site());

    let body = serialize_body(input);

    let generated = quote! {
        #[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
        const #dummy_const: () = {
            extern crate serde_conjure as _serde_conjure;

            #[automatically_derived]
            impl #impl_generics _serde_conjure::serde::Serialize for #ident #ty_generics
            #where_clause
            {
                fn serialize<__S>(
                    &self,
                    __serializer: __S
                ) -> _serde_conjure::serde::export::Result<__S::Ok, __S::Error>
                where
                    __S: _serde_conjure::serde::Serializer
                {
                    #body
                }
            }
        };
    };

    Ok(generated)
}

fn serialize_body(input: &syn::DeriveInput) -> TokenStream {
    match input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(ref fields),
            ..
        }) => serialize_struct_body(&fields),
        Data::Enum(ref e) => match EnumKind::of(e) {
            EnumKind::CLike => serialize_enum_body(&input.ident, e),
            EnumKind::Union => serialize_union_body(&input.ident, e),
        },
        Data::Struct(DataStruct {
            fields: Fields::Unnamed(_),
            ..
        }) => panic!("tuple structs are not supported"),
        Data::Struct(DataStruct {
            fields: Fields::Unit,
            ..
        }) => panic!("unit structs are not supported"),
        Data::Union(_) => panic!("unions are not supported"),
    }
}

fn serialize_struct_body(fields: &FieldsNamed) -> TokenStream {
    let len = fields.named.len();

    let serialize_fields = fields
        .named
        .iter()
        .map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let name = snake_to_camel(&ident.to_string());
            quote!{
                _serde_conjure::serde::ser::SerializeMap::serialize_entry(
                    &mut __serde_state,
                    &#name,
                    &self.#ident,
                )?;
            }
        }).collect::<Vec<_>>();

    quote! {
        let mut __serde_state =
            _serde_conjure::serde::Serializer::serialize_map(__serializer, Some(#len))?;
        #(#serialize_fields)*
        _serde_conjure::serde::ser::SerializeMap::end(__serde_state)
    }
}

fn serialize_enum_body(ty_name: &Ident, e: &DataEnum) -> TokenStream {
    let arms = e.variants.iter().map(|v| {
        let variant = &v.ident;
        let name = pascal_to_screaming(&variant.to_string());
        quote!(#ty_name::#variant => #name,)
    });

    quote! {
        let s = match *self {
            #(#arms)*
        };
        _serde_conjure::serde::ser::Serializer::serialize_str(__serializer, s)
    }
}

fn serialize_union_body(ty_name: &Ident, e: &DataEnum) -> TokenStream {
    let arms = e.variants.iter().map(|v| {
        let variant = &v.ident;
        let name = pascal_to_camel(&variant.to_string());
        quote! {
            #ty_name::#variant(ref __value) => {
                _serde_conjure::serde::ser::SerializeMap::serialize_entry(
                    &mut __serde_state,
                    &"type", &#name,
                )?;
                _serde_conjure::serde::ser::SerializeMap::serialize_entry(
                    &mut __serde_state,
                    &#name,
                    __value,
                )?;
            }
        }
    });

    quote! {
        let mut __serde_state =
            _serde_conjure::serde::Serializer::serialize_map(__serializer, Some(2))?;
        match *self {
            #(#arms)*
        }
        _serde_conjure::serde::ser::SerializeMap::end(__serde_state)
    }
}
