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
    parse_macro_input, parse_quote, Data, DeriveInput, Error, Fields, Ident, Token, TypeParamBound,
    TypePath,
};

fn main_crate() -> TokenStream2 {
    if env::var("CARGO_PKG_NAME").unwrap() == "unplug" {
        quote!(crate)
    } else {
        quote!(::unplug)
    }
}

enum KeyValueArg {
    Stream(Punctuated<TypeParamBound, Token![+]>),
    Error(TypePath),
}

impl Parse for KeyValueArg {
    fn parse(input: ParseStream<'_>) -> parse::Result<Self> {
        let key = Ident::parse(input)?;
        input.parse::<Token![=]>()?;
        match &*key.to_string() {
            "stream" => Ok(Self::Stream(Punctuated::parse_separated_nonempty(input)?)),
            "error" => Ok(Self::Error(TypePath::parse(input)?)),
            _ => Err(Error::new(key.span(), "unexpected parameter")),
        }
    }
}

struct AttributeArgs {
    stream: Option<Punctuated<TypeParamBound, Token![+]>>,
    error: Option<TypePath>,
}

impl Parse for AttributeArgs {
    fn parse(input: ParseStream<'_>) -> parse::Result<Self> {
        let (mut stream, mut error) = (None, None);
        let args = Punctuated::<KeyValueArg, Token![,]>::parse_terminated(input)?;
        for arg in args {
            match arg {
                KeyValueArg::Stream(s) => stream = Some(s),
                KeyValueArg::Error(e) => error = Some(e),
            }
        }
        Ok(Self { stream, error })
    }
}

#[proc_macro_derive(ReadFrom, attributes(read_from))]
pub fn derive_read_from(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let attr = input.attrs.iter().find(|a| a.path.is_ident("read_from"));
    let args = match attr {
        Some(attr) => match attr.parse_args::<AttributeArgs>() {
            Ok(args) => args,
            Err(err) => return err.to_compile_error().into(),
        },
        None => AttributeArgs { stream: None, error: None },
    };
    let bounds = args.stream.unwrap_or_else(|| parse_quote!(::std::io::Read));
    let error = args.error.unwrap_or_else(|| parse_quote!(::std::io::Error));

    let data = match &input.data {
        Data::Struct(data) => data,
        _ => {
            return Error::new(input.span(), "ReadFrom can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };
    let fields = match &data.fields {
        Fields::Named(named) => named,
        _ => {
            return Error::new(
                input.span(),
                "ReadFrom can only be derived for structs with named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    let name = input.ident;
    let unplug = main_crate();
    let field_name: Vec<Ident> = fields.named.iter().cloned().map(|f| f.ident.unwrap()).collect();
    // TODO: Generics support?
    let tokens = quote! {
        impl<R: #bounds> #unplug::common::ReadFrom<R> for #name {
            type Error = #error;
            fn read_from(reader: &mut R) -> ::std::result::Result<Self, Self::Error> {
                Ok(Self {
                    #(#field_name: #unplug::common::ReadFrom::read_from(reader)?),*
                })
            }
        }
    };
    tokens.into()
}

#[proc_macro_derive(WriteTo, attributes(write_to))]
pub fn derive_write_to(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let attr = input.attrs.iter().find(|a| a.path.is_ident("write_to"));
    let args = match attr {
        Some(attr) => match attr.parse_args::<AttributeArgs>() {
            Ok(args) => args,
            Err(err) => return err.to_compile_error().into(),
        },
        None => AttributeArgs { stream: None, error: None },
    };
    let bounds = args.stream.unwrap_or_else(|| parse_quote!(::std::io::Write));
    let error = args.error.unwrap_or_else(|| parse_quote!(::std::io::Error));

    let data = match &input.data {
        Data::Struct(data) => data,
        _ => {
            return Error::new(input.span(), "WriteTo can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };
    let fields = match &data.fields {
        Fields::Named(named) => named,
        _ => {
            return Error::new(
                input.span(),
                "WriteTo can only be derived for structs with named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    let name = input.ident;
    let unplug = main_crate();
    let field_name: Vec<Ident> = fields.named.iter().cloned().map(|f| f.ident.unwrap()).collect();
    // TODO: Generics support?
    let tokens = quote! {
        impl<W: #bounds> #unplug::common::WriteTo<W> for #name {
            type Error = #error;
            fn write_to(&self, writer: &mut W) -> ::std::result::Result<(), Self::Error> {
                #(#unplug::common::WriteTo::write_to(&self.#field_name, writer)?;)*
                Ok(())
            }
        }
    };
    tokens.into()
}
