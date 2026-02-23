use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::{Data, DeriveInput, Error, Field, Generics, Ident, Result};

pub fn expand_derive_serialize(input: DeriveInput) -> Result<TokenStream> {
    let DeriveInput {
        ident, generics, ..
    } = &input;

    let _self = Ident::new("self", ident.span());
    let buf = Ident::new("buf", ident.span());
    let body = match input.data {
        Data::Struct(ref data) => serialize_body(&buf, &_self, data.fields.iter()),
        Data::Union(ref data) => serialize_body(&buf, &_self, data.fields.named.iter()),
        Data::Enum(_) => {
            return Err(Error::new(
                ident.span(),
                "Derive `Serialize` for Enum is not supported yet",
            ));
        }
    };

    Ok(serialize_impl(ident, generics, buf, body))
}

fn serialize_impl(
    ident: &Ident,
    generics: &Generics,
    buf: Ident,
    body: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    quote! {
        #[automatically_derived]
        impl #impl_generics audiowire_serde::Serialize for #ident #ty_generics #where_clause {
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
    let field = if let Some(ident) = field.ident.as_ref() {
        quote!(#name.#ident)
    } else {
        let index = Literal::usize_unsuffixed(index);
        quote!(#name.#index)
    };
    quote!(audiowire_serde::Serialize::serialize(&#field, #buf))
}
