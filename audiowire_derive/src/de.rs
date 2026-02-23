use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Error, Field, Fields, Generics, Ident, Result};

pub fn expand_derive_deserialize(input: DeriveInput) -> Result<TokenStream> {
    let DeriveInput {
        ident, generics, ..
    } = &input;

    let buf = Ident::new("buf", ident.span());
    let body = match input.data {
        syn::Data::Struct(ref data) => {
            let named = matches!(data.fields, Fields::Named(_));
            deserialize_fields(&buf, data.fields.iter(), named)
        }
        syn::Data::Union(ref data) => deserialize_fields(&buf, data.fields.named.iter(), true),
        syn::Data::Enum(_) => {
            return Err(Error::new(
                ident.span(),
                "Derive `Deserialize` for Enum is not supported",
            ));
        }
    };

    Ok(deserialize_impl(ident, generics, buf, body))
}

fn deserialize_impl<'a>(
    ident: &Ident,
    generics: &Generics,
    buf: Ident,
    body: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    quote! {
        #[automatically_derived]
        impl #impl_generics audiowire_serde::Deserialize for #ident #ty_generics #where_clause {
            fn deserialize(#buf: &mut impl bytes::Buf) -> Result<Self, bytes::TryGetError> {
                Ok(#body)
            }
        }
    }
}

fn deserialize_fields<'a>(
    buf: &Ident,
    fields: impl Iterator<Item = &'a Field>,
    named: bool,
) -> TokenStream {
    let exprs = fields.map(|f| deserialize_buf(buf, f));
    if named {
        quote! {
            Self {
                #(
                    #exprs?,
                )*
            }
        }
    } else {
        quote!(Self(#(#exprs?, )*))
    }
}

fn deserialize_buf(buf: &Ident, field: &Field) -> TokenStream {
    let ty = &field.ty;
    let expr = quote!(<#ty as audiowire_serde::Deserialize>::deserialize(#buf));
    if let Some(ident) = field.ident.as_ref() {
        quote!(#ident: #expr)
    } else {
        expr
    }
}
