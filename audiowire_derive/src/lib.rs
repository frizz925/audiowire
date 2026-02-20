extern crate proc_macro;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use syn::{DeriveInput, Error, parse_macro_input};

mod de;
mod ser;

#[proc_macro_derive(Serialize)]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    ser::expand_derive_serialize(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Deserialize)]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    de::expand_derive_deserialize(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}
