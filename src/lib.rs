extern crate proc_macro;
use std::{sync::{Arc, Mutex}, collections::HashMap};
use syn::{parse_macro_input, ItemFn, FnArg, Pat, __private::ToTokens};

use proc_macro::TokenStream;
use lazy_static::lazy_static;

lazy_static! {
    static ref ENUM_ENTRIES_MAP: Arc<Mutex<HashMap<String, String>>> = {
        Arc::new(Mutex::new(HashMap::new()))
    };

    static ref ENUM_FUNCTION_DECLARATIONS: Arc<Mutex<HashMap<String, String>>> = {
        Arc::new(Mutex::new(HashMap::new()))
    };
    static ref MIDDLEWARE: Arc<Mutex<Vec<String>>> = {
        Arc::new(Mutex::new(vec![]))
    };
}


#[proc_macro_attribute]
pub fn middleware(_here: TokenStream, item: TokenStream) -> TokenStream {
    MIDDLEWARE.lock().unwrap().push(item.to_string());

    let clone = format!("#[allow(dead_code)]{}", item.to_string());
    clone.parse().unwrap()
}


#[proc_macro_attribute]
pub fn distributable(_here: TokenStream, item: TokenStream) -> TokenStream {
    let item_clone = item.clone();
    let item = parse_macro_input!(item as ItemFn);

    let mut lock = ENUM_ENTRIES_MAP.lock().unwrap();
    let mut types: Vec<String> = vec![];

    for i in item.sig.inputs.iter() {
        if let FnArg::Typed(t) = i {
            types.push(t.ty.to_token_stream().to_string());
        };
    };
    lock.insert(item.sig.ident.to_string(), types.join(","));

    let mut lock = ENUM_FUNCTION_DECLARATIONS.lock().unwrap();
    lock.insert(item.sig.ident.to_string(), item.to_token_stream().to_string());

    let clone = format!("#[allow(dead_code)]{}", item_clone.to_string());
    clone.parse().unwrap()
}

#[proc_macro]
pub fn build(_: TokenStream) -> TokenStream {
    let lock = ENUM_ENTRIES_MAP.lock().unwrap();

    let mut entries: Vec<String> = vec![];
    let mut tk = TokenStream::new();

    for entry in lock.iter() {
        entries.push(format!("{}({})", entry.0, entry.1));
    };


    let token: TokenStream = format!("enum Distributable {{ {} }}", entries.join(",")).parse().unwrap();
    tk.extend(token);

    let mut branches = vec![];

    for entry in ENUM_FUNCTION_DECLARATIONS.lock().unwrap().iter() {
        let item: TokenStream = entry.1.parse().unwrap();
        let item = parse_macro_input!(item as ItemFn);

        let mut args = vec![];

        for arg in item.sig.inputs.iter() {
            if let FnArg::Typed(t) = arg {
                if let Pat::Ident(ident) = &*t.pat {
                    args.push(ident.ident.to_string());
                };

            }
        }

        println!("{:?}", args);

        let branch = format!("Distributable::{}({}) => {},", entry.0, args.join(", "), item.block.to_token_stream().to_string());
        branches.push(branch);
    }
    
    let middleware = MIDDLEWARE.lock().unwrap();


    let mut names = vec![];
    for entry in middleware.iter() {
        let entry = entry.parse().unwrap();
        names.push(parse_macro_input!(entry as ItemFn).sig.ident.to_string() + "(b);");
    }


    let match_string: TokenStream = format!("fn redirect_to_function(d: Distributable, b: String) {{ {} {} match d {{ {} }} }}",
        middleware.join("\n"),
        names.join("\n"),
        branches.join("\n")
        ).parse().unwrap();

    println!("{}", match_string);

    tk.extend(match_string);

    tk
}
