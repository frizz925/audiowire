use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Error, Field, Ident, Result};

pub fn expand_derive_serialize(input: DeriveInput) -> Result<TokenStream> {
    let name = &input.ident;

    let _self = Ident::new("self", name.span());
    let buf = Ident::new("buf", name.span());
    let body = match input.data {
        syn::Data::Struct(ref data) => serialize_body(&buf, &_self, data.fields.iter()),
        syn::Data::Union(ref data) => serialize_body(&buf, &_self, data.fields.named.iter()),
        syn::Data::Enum(_) => {
            return Err(Error::new(
                name.span(),
                "Derive `Serialize` for Enum is not supported yet",
            ));
        }
    };

    Ok(serialize_impl(name, buf, body))
}

fn serialize_impl(name: &Ident, buf: Ident, body: TokenStream) -> TokenStream {
    quote! {
        impl audiowire_serde::Serialize for #name {
            fn serialize(&self, #buf: &mut impl bytes::BufMut) {
                #body
            }
        }
    }
}

fn serialize_body<'a>(
    buf: &Ident,
    name: &Ident,
    fields: impl Iterator<Item = &'a Field>,
) -> TokenStream {
    let exprs = fields
        .enumerate()
        .map(|(i, f)| serialize_buf(buf, name, f, i));
    quote! {
        #(
            #exprs;
        )*
    }
}

fn serialize_buf(buf: &Ident, name: &Ident, field: &Field, index: usize) -> TokenStream {
    let field_expr = if let Some(ident) = field.ident.as_ref() {
        quote!(#name.#ident)
    } else {
        quote!(#name.#index)
    };
    quote!(audiowire_serde::Serialize::serialize(&#field_expr, #buf))
}
