#![recursion_limit = "256"]

extern crate proc_macro;
extern crate proc_macro2;
#[macro_use]
extern crate quote;
#[macro_use]
extern crate syn;

use proc_macro::TokenStream;
use syn::{DataEnum, Fields};

mod bound;
mod de;
mod ser;

#[proc_macro_derive(ConjureSerialize)]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let input = syn::parse(input).unwrap();
    match ser::expand_derive_serialize(&input) {
        Ok(expanded) => expanded.into(),
        Err(msg) => panic!(msg),
    }
}

#[proc_macro_derive(ConjureDeserialize)]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = syn::parse(input).unwrap();
    match de::expand_derive_deserialize(&input) {
        Ok(expanded) => expanded.into(),
        Err(msg) => panic!(msg),
    }
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

fn pascal_to_screaming(name: &str) -> String {
    let mut out = String::new();
    let mut first = true;
    for ch in name.chars() {
        if !first && ch.is_uppercase() {
            out.push('_');
        }
        out.push(ch.to_ascii_uppercase());
        first = false;
    }
    out
}

fn pascal_to_camel(name: &str) -> String {
    let mut out = String::new();
    let mut first = true;
    for ch in name.chars() {
        let ch = if first {
            first = false;
            ch.to_ascii_lowercase()
        } else {
            ch
        };
        out.push(ch);
    }
    out
}

enum EnumKind {
    CLike,
    Union,
}

impl EnumKind {
    fn of(e: &DataEnum) -> EnumKind {
        if e.variants.iter().all(|v| match v.fields {
            Fields::Unit => true,
            _ => false,
        }) {
            EnumKind::CLike
        } else if e.variants.iter().all(|v| match v.fields {
            Fields::Unnamed(ref u) if u.unnamed.len() == 1 => true,
            _ => false,
        }) {
            EnumKind::Union
        } else {
            panic!(
                "the variants of an enum must either be entirely unit-like or entirely newtype-like"
            );
        }
    }
}
