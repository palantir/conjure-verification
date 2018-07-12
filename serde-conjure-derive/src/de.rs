use proc_macro2::{Span, TokenStream};
use syn::{
    Data, DataEnum, DataStruct, DeriveInput, Fields, FieldsNamed, GenericParam, Ident,
    ImplGenerics, Lifetime, LifetimeDef, TypeGenerics, WhereClause,
};

use bound;
use {pascal_to_camel, pascal_to_screaming, snake_to_camel, EnumKind};

pub fn expand_derive_deserialize(input: &DeriveInput) -> Result<TokenStream, String> {
    let ident = &input.ident;

    let generics = bound::without_defaults(&input.generics);
    let generics = bound::with_bound(
        &generics,
        &parse_quote!(_serde_conjure::serde::Deserialize<'de>),
    );

    let (_, ty_generics, where_clause) = generics.split_for_impl();

    let mut impl_generics = generics.clone();
    impl_generics
        .params
        .push(GenericParam::Lifetime(LifetimeDef::new(Lifetime::new(
            "'de",
            Span::call_site(),
        ))));

    let (impl_generics, _, _) = impl_generics.split_for_impl();

    let dummy_const = Ident::new(
        &format!("_IMPL_DESERIALIZE_FOR_{}", ident),
        Span::call_site(),
    );

    let body = deserialize_body(input, &impl_generics, &ty_generics, where_clause);

    let generated = quote! {
        #[allow(non_upper_case_globals, unused_attributes, unused_qualifications,
                non_camel_case_types, unreachable_patterns)]
        const #dummy_const: () = {
            extern crate serde_conjure as _serde_conjure;

            #[automatically_derived]
            impl #impl_generics _serde_conjure::serde::Deserialize<'de> for #ident #ty_generics
            #where_clause
            {
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde_conjure::serde::export::Result<Self, __D::Error>
                where
                    __D: _serde_conjure::serde::Deserializer<'de>
                {
                    #body
                }
            }
        };
    };

    Ok(generated)
}

fn deserialize_body(
    input: &DeriveInput,
    impl_generics: &ImplGenerics,
    ty_generics: &TypeGenerics,
    where_clause: Option<&WhereClause>,
) -> TokenStream {
    match input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(ref fields),
            ..
        }) => deserialize_struct_body(
            &input.ident,
            fields,
            impl_generics,
            ty_generics,
            where_clause,
        ),
        Data::Enum(ref e) => match EnumKind::of(e) {
            EnumKind::CLike => deserialize_enum_body(&input.ident, e),
            EnumKind::Union => {
                deserialize_union_body(&input.ident, e, impl_generics, ty_generics, where_clause)
            }
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

fn deserialize_struct_body(
    ty_name: &Ident,
    fields: &FieldsNamed,
    impl_generics: &ImplGenerics,
    ty_generics: &TypeGenerics,
    where_clause: Option<&WhereClause>,
) -> TokenStream {
    let struct_fields = deserialize_struct_fields(fields);

    let expecting = format!("struct {}", ty_name);

    let option_decls = fields.named.iter().map(|f| {
        let var = Ident::new(
            &format!("__field_{}", f.ident.as_ref().unwrap()),
            Span::call_site(),
        );
        quote!(let mut #var = None;)
    });

    let value_arms = fields.named.iter().map(|f| {
        let ident = f.ident.as_ref().unwrap();
        let name = snake_to_camel(&ident.to_string());
        let var = Ident::new(&format!("__field_{}", ident), Span::call_site());
        quote! {
            __Field::#ident => {
                if _serde_conjure::serde::export::Option::is_some(&#var) {
                    return Err(_serde_conjure::serde::de::Error::duplicate_field(#name));
                }
                #var = _serde_conjure::serde::export::Some(
                    _serde_conjure::serde::de::MapAccess::next_value(&mut __map)?,
                );
            }
        }
    });

    let var_extracts = fields.named.iter().map(|f| {
        let ident = f.ident.as_ref().unwrap();
        let name = snake_to_camel(&ident.to_string());
        let var = Ident::new(&format!("__field_{}", ident), Span::call_site());

        quote! {
            let #var = match #var {
                _serde_conjure::serde::export::Some(#var) => #var,
                _serde_conjure::serde::export::None => _serde_conjure::missing_field(#name)?,
            };
        }
    });

    let vars = fields.named.iter().map(|f| {
        let ident = f.ident.as_ref().unwrap();
        let var = Ident::new(&format!("__field_{}", ident), Span::call_site());
        quote!(#ident: #var)
    });

    quote! {
        #struct_fields

        struct __Visitor #ty_generics(
            _serde_conjure::serde::export::PhantomData<#ty_name #ty_generics>
        );

        impl #impl_generics _serde_conjure::serde::de::Visitor<'de> for __Visitor #ty_generics
        #where_clause
        {
            type Value = #ty_name #ty_generics;

            fn expecting(
                &self,
                fmt: &mut _serde_conjure::serde::export::Formatter
            ) -> _serde_conjure::serde::export::fmt::Result {
                _serde_conjure::serde::export::Formatter::write_str(fmt, #expecting)
            }

            #[inline]
            fn visit_map<__A>(
                self,
                mut __map: __A
            ) -> _serde_conjure::serde::export::Result<Self::Value, __A::Error>
            where
                __A: _serde_conjure::serde::de::MapAccess<'de>
            {
                #(#option_decls)*

                while let _serde_conjure::serde::export::Some(__key) =
                    _serde_conjure::serde::de::MapAccess::next_key::<__Field>(&mut __map)?
                {
                    match __key {
                        #(#value_arms)*
                        __Field::__ignore => {
                            let _ = _serde_conjure::serde::de::MapAccess::next_value::
                                <_serde_conjure::serde::de::IgnoredAny>(&mut __map)?;
                        }
                    }
                }

                #(#var_extracts)*

                _serde_conjure::serde::export::Ok(#ty_name {
                    #(#vars,)*
                })
            }
        }

        let __visitor = __Visitor(_serde_conjure::serde::export::PhantomData);
        _serde_conjure::serde::Deserializer::deserialize_map(__deserializer, __visitor)
    }
}

fn deserialize_struct_fields(fields: &FieldsNamed) -> TokenStream {
    let names = fields.named.iter().map(|f| {
        let ident = f.ident.as_ref().unwrap();
        snake_to_camel(&ident.to_string())
    });

    let variants = fields.named.iter().map(|f| {
        let ident = f.ident.as_ref().unwrap();
        quote!(#ident)
    });

    let arms = fields.named.iter().map(|f| {
        let ident = f.ident.as_ref().unwrap();
        let name = snake_to_camel(&ident.to_string());
        quote!(#name => _serde_conjure::serde::export::Ok(__Field::#ident))
    });

    quote! {
        const FIELDS: &'static [&'static str] = &[#(#names),*];

        enum __Field {
            #(#variants,)*
            __ignore,
        }

        impl<'de> _serde_conjure::serde::Deserialize<'de> for __Field {
            fn deserialize<__D>(
                __deserializer: __D
            ) -> _serde_conjure::serde::export::Result<__Field, __D::Error>
            where
                __D: _serde_conjure::serde::Deserializer<'de>
            {
                _serde_conjure::serde::Deserializer::deserialize_str(__deserializer, __FieldVisitor)
            }
        }

        struct __FieldVisitor;

        impl<'de> _serde_conjure::serde::de::Visitor<'de> for __FieldVisitor {
            type Value = __Field;

            fn expecting(
                &self,
                fmt: &mut _serde_conjure::serde::export::Formatter
            ) -> _serde_conjure::serde::export::fmt::Result {
                _serde_conjure::serde::export::Formatter::write_str(fmt, "field name")
            }

            fn visit_str<__E>(
                self,
                __value: &str
            ) -> _serde_conjure::serde::export::Result<Self::Value, __E>
            where
                __E: _serde_conjure::serde::de::Error
            {
                match __value {
                    #(#arms,)*
                    _ => _serde_conjure::serde::export::Ok(__Field::__ignore),
                }
            }
        }
    }
}

fn deserialize_enum_body(ty_name: &Ident, e: &DataEnum) -> TokenStream {
    let expecting = format!("enum {}", ty_name);

    let arms = e.variants.iter().map(|v| {
        let variant = &v.ident;
        let name = pascal_to_screaming(&variant.to_string());
        quote!(#name => _serde_conjure::serde::export::Result::Ok(#ty_name::#variant),)
    });

    let names = e
        .variants
        .iter()
        .map(|v| pascal_to_screaming(&v.ident.to_string()));

    quote! {
        const VARIANTS: &'static [&'static str] = &[#(#names),*];

        struct __Visitor;

        impl<'de> _serde_conjure::serde::de::Visitor<'de> for __Visitor {
            type Value = #ty_name;

            fn expecting(
                &self,
                fmt: &mut _serde_conjure::serde::export::Formatter
            ) -> _serde_conjure::serde::export::fmt::Result {
                _serde_conjure::serde::export::Formatter::write_str(fmt, #expecting)
            }

            fn visit_str<__E>(
                self,
                __value: &str
            ) -> _serde_conjure::serde::export::Result<Self::Value, __E>
            where
                __E: _serde_conjure::serde::de::Error
            {
                match __value {
                    #(#arms)*
                    _ => _serde_conjure::serde::export::Err(
                        _serde_conjure::serde::de::Error::unknown_variant(__value, VARIANTS)
                    ),
                }
            }
        }

        _serde_conjure::serde::Deserializer::deserialize_str(__deserializer, __Visitor)
    }
}

fn deserialize_union_body(
    ty_name: &Ident,
    e: &DataEnum,
    impl_generics: &ImplGenerics,
    ty_generics: &TypeGenerics,
    where_clause: Option<&WhereClause>,
) -> TokenStream {
    let union_variants = deserialize_union_variants(e);

    let expecting = format!("union {}", ty_name);

    let type_first_arms = e.variants.iter().map(|v| {
        let ident = &v.ident;
        quote! {
            (__Variant::#ident, _serde_conjure::serde::export::Some(__Variant::#ident)) => {
                let __value = _serde_conjure::serde::de::MapAccess::next_value(&mut __map)?;
                _serde_conjure::serde::export::Ok(#ty_name::#ident(__value))
            }
        }
    });

    let data_first_arms = e.variants.iter().map(|v| {
        let ident = &v.ident;
        quote! {
            __Variant::#ident => {
                let __value = _serde_conjure::serde::de::MapAccess::next_value(&mut __map)?;
                #ty_name::#ident(__value)
            }
        }
    });

    let variant_match_patterns = e.variants.iter().map(|v| {
        let ident = &v.ident;
        quote!((__Variant::#ident, __Variant::#ident))
    });

    quote! {
        #union_variants

        struct __Visitor #ty_generics(
            _serde_conjure::serde::export::PhantomData<#ty_name #ty_generics>,
        );

        impl #impl_generics _serde_conjure::serde::de::Visitor<'de> for __Visitor #ty_generics
        #where_clause
        {
            type Value = #ty_name #ty_generics;

            fn expecting(
                &self,
                fmt: &mut _serde_conjure::serde::export::Formatter,
            ) -> _serde_conjure::serde::export::fmt::Result {
                _serde_conjure::serde::export::Formatter::write_str(fmt, #expecting)
            }

            #[inline]
            fn visit_map<__A>(
                self,
                mut __map: __A
            ) -> _serde_conjure::serde::export::Result<Self::Value, __A::Error>
            where
                __A: _serde_conjure::serde::de::MapAccess<'de>
            {
                match _serde_conjure::serde::de::MapAccess::next_key::
                    <_serde_conjure::UnionField<__Variant>>(&mut __map)?
                {
                    _serde_conjure::serde::export::Some(_serde_conjure::UnionField::Type) => {
                        let __variant = _serde_conjure::serde::de::MapAccess::next_value::
                            <__Variant>(&mut __map)?;
                        let __key = _serde_conjure::serde::de::MapAccess::next_key::
                            <__Variant>(&mut __map)?;
                        match (__variant, __key) {
                            #(#type_first_arms,)*
                            (__variant, _serde_conjure::serde::export::Some(__key)) => {
                                _serde_conjure::serde::export::Err(
                                    _serde_conjure::serde::de::Error::invalid_value(
                                        _serde_conjure::serde::de::Unexpected::Str(
                                            __Variant::field(&__key),
                                        ),
                                        &__Variant::field(&__variant),
                                    ),
                                )
                            }
                            (__variant, _serde_conjure::serde::export::None) => {
                                _serde_conjure::serde::export::Err(
                                    _serde_conjure::serde::de::Error::missing_field(
                                        __Variant::field(&__variant),
                                    )
                                )
                            }
                        }
                    }
                    _serde_conjure::serde::export::Some(
                        _serde_conjure::UnionField::Data(__variant),
                    ) => {
                        let __value = match __variant {
                            #(#data_first_arms,)*
                        };

                        _serde_conjure::serde::de::MapAccess::next_key::
                            <_serde_conjure::UnionTypeField>(&mut __map)?;
                        let __type_variant =
                            _serde_conjure::serde::de::MapAccess::next_value::
                                <__Variant>(&mut __map)?;
                        match (__variant, __type_variant) {
                            #(#variant_match_patterns)|* => {
                                _serde_conjure::serde::export::Ok(__value)
                            }
                            (__variant, __type_variant) => {
                                _serde_conjure::serde::export::Err(
                                    _serde_conjure::serde::de::Error::invalid_value(
                                        _serde_conjure::serde::de::Unexpected::Str(
                                            __Variant::field(&__type_variant),
                                        ),
                                        &__Variant::field(&__variant),
                                    )
                                )
                            }
                        }
                    }
                    _serde_conjure::serde::export::None => {
                        Err(_serde_conjure::serde::de::Error::missing_field("type"))
                    }
                }
            }
        }

        let __visitor = __Visitor(_serde_conjure::serde::export::PhantomData);
        _serde_conjure::serde::Deserializer::deserialize_map(__deserializer, __visitor)
    }
}

fn deserialize_union_variants(e: &DataEnum) -> TokenStream {
    let names = e
        .variants
        .iter()
        .map(|v| pascal_to_camel(&v.ident.to_string()));

    let field_arms = e.variants.iter().map(|v| {
        let ident = &v.ident;
        let name = pascal_to_camel(&ident.to_string());
        quote!(__Variant::#ident => #name)
    });

    let arms = e.variants.iter().map(|v| {
        let ident = &v.ident;
        let name = pascal_to_camel(&ident.to_string());
        quote!(#name => _serde_conjure::serde::export::Ok(__Variant::#ident))
    });

    let variants = e.variants.iter().map(|v| {
        let ident = &v.ident;
        quote!(#ident)
    });

    quote! {
        const VARIANTS: &'static [&'static str] = &[#(#names),*];

        enum __Variant {
            #(#variants,)*
        }

        impl __Variant {
            fn field(&self) -> &'static str {
                match *self {
                    #(#field_arms,)*
                }
            }
        }

        impl<'de> _serde_conjure::serde::Deserialize<'de> for __Variant {
            fn deserialize<__D>(
                __deserializer: __D
            ) -> _serde_conjure::serde::export::Result<__Variant, __D::Error>
            where
                __D: _serde_conjure::serde::Deserializer<'de>
            {
                _serde_conjure::serde::Deserializer::deserialize_str(
                    __deserializer,
                    __VariantVisitor,
                )
            }
        }

        struct __VariantVisitor;

        impl<'de> _serde_conjure::serde::de::Visitor<'de> for __VariantVisitor {
            type Value = __Variant;

            fn expecting(
                &self,
                fmt: &mut _serde_conjure::serde::export::Formatter
            ) -> _serde_conjure::serde::export::fmt::Result {
                _serde_conjure::serde::export::Formatter::write_str(fmt, "variant name")
            }

            fn visit_str<__E>(
                self,
                __value: &str,
            ) -> _serde_conjure::serde::export::Result<Self::Value, __E>
            where
                __E: _serde_conjure::serde::de::Error
            {
                match __value {
                    #(#arms,)*
                    _ => Err(_serde_conjure::serde::de::Error::unknown_variant(__value, VARIANTS)),
                }
            }
        }
    }
}
