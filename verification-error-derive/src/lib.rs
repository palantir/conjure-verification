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
#![recursion_limit = "256"]

extern crate proc_macro;
extern crate proc_macro2;
extern crate syn;

#[macro_use]
extern crate quote;

use proc_macro2::{Span, TokenStream};
use syn::{Attribute, Data, DataEnum, DeriveInput, Fields, Ident, Lit, Meta, NestedMeta};

#[proc_macro_derive(ErrorType, attributes(error_type))]
pub fn derive_error_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse(input).unwrap();
    match expand_derive_error_type(&input) {
        Ok(expanded) => expanded.into(),
        Err(msg) => panic!(msg),
    }
}

fn expand_derive_error_type(input: &DeriveInput) -> Result<TokenStream, String> {
    let ident = &input.ident;
    let dummy_const = Ident::new(
        &format!("_IMPL_ERROR_TYPE_FOR_{}", ident),
        Span::call_site(),
    );

    let data = match input.data {
        Data::Enum(ref data) => data,
        Data::Struct(_) | Data::Union(_) => return Err("expected an enum".to_string()),
    };

    let namespace = namespace(input)?;
    let code_body = code(ident, data)?;
    let name_body = name(ident, data);
    let safe_params_body = body_params(ident, data, true)?;
    let unsafe_params_body = body_params(ident, data, false)?;
    let parse_body = parse(input, data)?;

    let generated = quote!{
        #[allow(non_upper_case_globals)]
        const #dummy_const: () = {
            extern crate conjure_verification_error as _conjure_verification_error;

            impl _conjure_verification_error::ErrorType for #ident {
                const NAMESPACE: &'static str = #namespace;

                fn code(&self) -> _conjure_verification_error::Code {
                    #code_body
                }

                fn name(&self) -> &'static str {
                    #name_body
                }

                fn safe_params(&self) -> ::std::collections::HashMap<&'static str, String> {
                    #safe_params_body
                }

                fn unsafe_params(&self) -> ::std::collections::HashMap<&'static str, String> {
                    #unsafe_params_body
                }

                fn parse(error: &_conjure_verification_error::SerializableError) -> ::std::option::Option<Self> {
                    #parse_body
                }
            }
        };
    };

    Ok(generated)
}

fn string_attr(attrs: &[Attribute], target: &str) -> Result<String, String> {
    for attr in attrs {
        let attr = match attr.interpret_meta() {
            Some(attr) => attr,
            None => continue,
        };

        if attr.name() != "error_type" {
            continue;
        }

        let list = match attr {
            Meta::List(ref list) => list,
            _ => return Err("expected #[error_type(...)]".to_string()),
        };

        for item in &list.nested {
            match *item {
                NestedMeta::Meta(Meta::NameValue(ref meta)) if meta.ident == target => {
                    let value = match meta.lit {
                        Lit::Str(ref s) => s,
                        _ => return Err("expected a string literal".to_string()),
                    };

                    return Ok(value.value());
                }
                _ => {}
            }
        }
    }

    Err(format!(
        "expected a #[error_type({} = ...)] attribute",
        target
    ))
}

fn unit_attr(attrs: &[Attribute], target: &str) -> Result<bool, String> {
    for attr in attrs {
        let attr = match attr.interpret_meta() {
            Some(attr) => attr,
            None => continue,
        };

        if attr.name() != "error_type" {
            continue;
        }

        let list = match attr {
            Meta::List(list) => list,
            _ => return Err("expected #[error_type(...)]".to_string()),
        };

        for item in &list.nested {
            match *item {
                NestedMeta::Meta(Meta::Word(ref name)) if name == target => {
                    return Ok(true);
                }
                _ => {}
            }
        }
    }

    Ok(false)
}

fn namespace(input: &DeriveInput) -> Result<TokenStream, String> {
    string_attr(&input.attrs, "namespace").map(|s| quote!(#s))
}

fn code(ident: &Ident, data: &DataEnum) -> Result<TokenStream, String> {
    let arms = data
        .variants
        .iter()
        .map(|v| {
            let variant = &v.ident;
            let pattern = match v.fields {
                Fields::Named(_) => quote!(#ident::#variant { .. }),
                Fields::Unnamed(_) => quote!(#ident::#variant(..)),
                Fields::Unit => quote!(#ident::#variant),
            };
            let code = string_attr(&v.attrs, "code")?;
            let code = Ident::new(&code, Span::call_site());
            Ok(quote!(#pattern => _conjure_verification_error::Code::#code))
        }).collect::<Result<Vec<_>, String>>()?;

    let generated = quote! {
        match *self {
            #(#arms,)*
        }
    };

    Ok(generated)
}

fn name(ident: &Ident, data: &DataEnum) -> TokenStream {
    let arms = data.variants.iter().map(|v| {
        let variant = &v.ident;
        let pattern = match v.fields {
            Fields::Named(_) => quote!(#ident::#variant { .. }),
            Fields::Unnamed(_) => quote!(#ident::#variant(..)),
            Fields::Unit => quote!(#ident::#variant),
        };
        let name = variant.to_string();
        quote!(#pattern => #name)
    });

    let generated = quote! {
        match *self {
            #(#arms,)*
        }
    };

    generated
}

fn body_params(ident: &Ident, data: &DataEnum, safe: bool) -> Result<TokenStream, String> {
    let arms = data
        .variants
        .iter()
        .map(|v| {
            let variant = &v.ident;
            let generated = match v.fields {
                Fields::Named(ref fields) => {
                    let field_bindings = fields.named.iter().map(|f| {
                        let name = f.ident.as_ref().unwrap();
                        let binding = Ident::new(&format!("_field_{}", name), Span::call_site());
                        quote!(#name: ref #binding)
                    });

                    let mut filtered_fields = vec![];
                    for field in &fields.named {
                        if unit_attr(&field.attrs, "safe")? == safe {
                            filtered_fields.push(field);
                        }
                    }

                    let field_inserts = filtered_fields.iter().map(|f| {
                        let name = f.ident.as_ref().unwrap();
                        let key = snake_to_camel(&name.to_string());
                        let binding = Ident::new(&format!("_field_{}", name), Span::call_site());
                        quote!(_map.insert(#key, #binding.to_string()))
                    });

                    quote! {
                        #ident::#variant { #(#field_bindings,)* } => {
                            let mut _map = ::std::collections::HashMap::new();
                            #(#field_inserts;)*
                            _map
                        }
                    }
                }
                Fields::Unnamed(_) => return Err("expected a struct or unit variant".to_string()),
                Fields::Unit => quote!(#ident::#variant => ::std::collections::HashMap::new()),
            };

            Ok(generated)
        }).collect::<Result<Vec<_>, String>>()?;

    let generated = quote! {
        match *self {
            #(#arms,)*
        }
    };

    Ok(generated)
}

fn parse(input: &DeriveInput, data: &DataEnum) -> Result<TokenStream, String> {
    let namespace = string_attr(&input.attrs, "namespace")?;
    let ident = &input.ident;

    let arms = data
        .variants
        .iter()
        .map(|v| {
            let variant = &v.ident;
            let code = format!("{}:{}", namespace, variant.to_string());

            let args = match v.fields {
                Fields::Named(ref fields) => fields.named.len(),
                Fields::Unnamed(_) => return Err("expected a struct or unit variant".to_string()),
                Fields::Unit => 0,
            };

            let build = match v.fields {
                Fields::Named(ref fields) => {
                    let fields = fields.named.iter().map(|f| {
                        let ident = f.ident.as_ref().unwrap();
                        let param = snake_to_camel(&ident.to_string());

                        quote!(#ident: _params.get(#param).and_then(|s| s.parse().ok())?)
                    });

                    quote! {
                        #ident::#variant {
                            #(#fields,)*
                        }
                    }
                }
                Fields::Unnamed(_) => unreachable!(),
                Fields::Unit => quote!(#ident::#variant),
            };

            let generated = quote! {
                #code => {
                    if _params.len() != #args {
                        return ::std::option::Option::None;
                    }

                    ::std::option::Option::Some(#build)
                }
            };

            Ok(generated)
        }).collect::<Result<Vec<_>, _>>()?;

    let generated = quote! {
        let _params = error.params();
        match error.name() {
            #(#arms,)*
            _ => ::std::option::Option::None,
        }
    };

    Ok(generated)
}

fn snake_to_camel(name: &str) -> String {
    let mut out = String::new();
    let mut caps = false;
    for ch in name.chars() {
        if ch == '_' {
            caps = true;
        } else {
            let ch = if caps {
                caps = false;
                ch.to_ascii_uppercase()
            } else {
                ch
            };
            out.push(ch);
        }
    }
    out
}
