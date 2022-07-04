#![warn(
    absolute_paths_not_starting_with_crate,
    elided_lifetimes_in_paths,
    explicit_outlives_requirements,
    trivial_casts,
    trivial_numeric_casts,
    unconditional_recursion,
    unreachable_patterns,
    unreachable_pub,
    unused_import_braces,
    unused_lifetimes,
    unused_must_use,
    unused_qualifications,
    variant_size_differences
)]

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::env;
use syn::parse::{self, Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, parse_quote, Data, DeriveInput, Error, Fields, Ident, Token, TypePath,
};

fn main_crate() -> TokenStream2 {
    if env::var("CARGO_PKG_NAME").unwrap() == "unplug" {
        quote!(crate)
    } else {
        quote!(::unplug)
    }
}

enum KeyValueArg {
    Error(TypePath),
}

impl Parse for KeyValueArg {
    fn parse(input: ParseStream<'_>) -> parse::Result<Self> {
        let key = Ident::parse(input)?;
        input.parse::<Token![=]>()?;
        match &*key.to_string() {
            "error" => Ok(Self::Error(TypePath::parse(input)?)),
            _ => Err(Error::new(key.span(), "unexpected parameter")),
        }
    }
}

struct AttributeArgs {
    error: Option<TypePath>,
}

impl Parse for AttributeArgs {
    fn parse(input: ParseStream<'_>) -> parse::Result<Self> {
        let mut error = None;
        let args = Punctuated::<KeyValueArg, Token![,]>::parse_terminated(input)?;
        for arg in args {
            match arg {
                KeyValueArg::Error(e) => error = Some(e),
            }
        }
        Ok(Self { error })
    }
}

#[proc_macro_derive(SerializeEvent, attributes(serialize))]
pub fn derive_serialize_event(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let unplug = main_crate();
    let serialize = quote!(#unplug::event::serialize);

    let attr = input.attrs.iter().find(|a| a.path.is_ident("serialize"));
    let args = match attr {
        Some(attr) => match attr.parse_args::<AttributeArgs>() {
            Ok(args) => args,
            Err(err) => return err.to_compile_error().into(),
        },
        None => AttributeArgs { error: None },
    };
    let error = args.error.unwrap_or_else(|| parse_quote!(#serialize::Error));

    let data = match &input.data {
        Data::Struct(data) => data,
        _ => {
            return Error::new(input.span(), "SerializeEvent can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };
    let fields = match &data.fields {
        Fields::Named(named) => named,
        _ => {
            return Error::new(
                input.span(),
                "SerializeEvent can only be derived for structs with named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    let name = input.ident;
    let field_name: Vec<Ident> = fields.named.iter().cloned().map(|f| f.ident.unwrap()).collect();
    let tokens = quote! {
        impl #serialize::SerializeEvent for #name {
            type Error = #error;
            fn serialize(&self, ser: &mut dyn #serialize::EventSerializer) -> ::std::result::Result<(), Self::Error> {
                #(#serialize::SerializeEvent::serialize(&self.#field_name, ser)?;)*
                Ok(())
            }
        }
    };
    tokens.into()
}

#[proc_macro_derive(DeserializeEvent, attributes(serialize))]
pub fn derive_deserialize_event(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let unplug = main_crate();
    let serialize = quote!(#unplug::event::serialize);

    let attr = input.attrs.iter().find(|a| a.path.is_ident("serialize"));
    let args = match attr {
        Some(attr) => match attr.parse_args::<AttributeArgs>() {
            Ok(args) => args,
            Err(err) => return err.to_compile_error().into(),
        },
        None => AttributeArgs { error: None },
    };
    let error = args.error.unwrap_or_else(|| parse_quote!(#serialize::Error));

    let data = match &input.data {
        Data::Struct(data) => data,
        _ => {
            return Error::new(input.span(), "DeserializeEvent can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };
    let fields = match &data.fields {
        Fields::Named(named) => named,
        _ => {
            return Error::new(
                input.span(),
                "DeserializeEvent can only be derived for structs with named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    let name = input.ident;
    let field_name: Vec<Ident> = fields.named.iter().cloned().map(|f| f.ident.unwrap()).collect();
    let tokens = quote! {
        impl #serialize::DeserializeEvent for #name {
            type Error = #error;
            fn deserialize(de: &mut dyn #serialize::EventDeserializer) -> ::std::result::Result<Self, Self::Error> {
                Ok(Self {
                    #(#field_name: #serialize::DeserializeEvent::deserialize(de)?),*
                })
            }
        }
    };
    tokens.into()
}
