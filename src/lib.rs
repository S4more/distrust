extern crate proc_macro;
use std::{sync::{Arc, Mutex}, collections::HashMap};
use syn::{parse_macro_input, ItemFn, __private::ToTokens, FnArg};

use proc_macro::TokenStream;
use lazy_static::lazy_static;

lazy_static! {
    static ref ENUM_ENTRIES_MAP: Arc<Mutex<HashMap<String, String>>> = {
        Arc::new(Mutex::new(HashMap::new()))
    };
}

#[proc_macro_attribute]
pub fn distributable(_: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as ItemFn);
    let item_clone = item.clone();

    let mut lock = ENUM_ENTRIES_MAP.lock().unwrap();
    let mut types: Vec<String> = vec![];

    for i in item.sig.inputs.iter() {
        if let FnArg::Typed(t) = i {
            types.push(t.ty.to_token_stream().to_string());
        };
    };
    lock.insert(item.sig.ident.to_string(), types.join(","));

    item_clone.to_token_stream().into()
}

#[proc_macro]
pub fn build(_item: TokenStream) -> TokenStream {
    let lock = ENUM_ENTRIES_MAP.lock().unwrap();
    let mut entries: Vec<String> = vec![];

    for entry in lock.iter() {
        entries.push(format!("{}({})", entry.0, entry.1));
    };

    format!("enum Distributable {{ {} }}", entries.join(",")).parse().unwrap()
}
