#![feature(proc_macro_quote)]
extern crate proc_macro;
use std::{sync::{Arc, Mutex}, str::FromStr};
use quote::format_ident;
use syn::{ItemFn, FnArg, __private::ToTokens, Pat, parse_str, ExprTuple, Expr, ReturnType};

use proc_macro2::{TokenStream, Ident, Span};
use lazy_static::lazy_static;
use convert_case::{Case, Casing};

lazy_static! {
    static ref FUNCTIONS: Arc<Mutex<Vec<DistributableFunction>>> = {
        Arc::new(Mutex::new(vec![]))
    };

    static ref MIDDLEWARE: Arc<Mutex<Vec<DistributableFunction>>> = {
        Arc::new(Mutex::new(vec![]))
    };
}

struct DistributableFunction {
    name: String,
    arguments: Vec<(String, String)>,
    raw: String,
    return_type: String
}

impl DistributableFunction {

    fn parse(stream: &proc_macro::TokenStream) -> ItemFn {
        let item: ItemFn = parse_str(stream.to_string().as_str()).unwrap();
        return item
    }

    fn new(stream: &proc_macro::TokenStream) -> Self {
        let mut types = vec![];
        let item = Self::parse(&stream);

        for i in item.sig.inputs.iter() {
            if let FnArg::Typed(t) = i {
                if let Pat::Ident(named_arg) = &*t.pat {
                    types.push((named_arg.ident.to_string(), t.ty.to_token_stream().to_string()));
                }
            };
        };
        
        let return_quote = match &item.sig.output {
            ReturnType::Default => quote::quote!(()),
            ReturnType::Type(_, b) => quote::quote!(#b),
        };

        let distributable_function = DistributableFunction {
            name: item.sig.ident.to_string(),
            arguments: types,
            raw: item.to_token_stream().to_string(),
            return_type:  return_quote.to_token_stream().to_string()
        };


        distributable_function
    }
}


#[proc_macro_attribute]
pub fn middleware(_here: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let distributable_function = DistributableFunction::new(&item);
    MIDDLEWARE.lock().unwrap().push(distributable_function);

    let item = DistributableFunction::parse(&item);

    quote::quote! {
        #[allow(dead_code)]
        #item
    }.into()
}


#[proc_macro_attribute]
pub fn distributable(_here: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut lock = FUNCTIONS.lock().unwrap();
    let distributable_function = DistributableFunction::new(&item);
    let item = DistributableFunction::parse(&item);

    lock.push(distributable_function);

    quote::quote! {
        #[allow(dead_code)]
        #item
    }.into()
}

#[proc_macro]
pub fn build(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let functions = FUNCTIONS.lock().unwrap();
    let mut tk = TokenStream::new();

    let function_names: Vec<Ident> = functions
        .iter()
        .map(|function| format_ident!("{}", function.name.clone().to_case(Case::Pascal)))
        .collect();

    let mut arg_types : Vec<Expr> = vec![];
    for function in functions.iter() {
        let arg_list: String = function
            .arguments
            .iter()
            .map(|arg| arg.1.clone())
            .collect::<Vec<String>>()
            .join(",");

        let arg_list = format!("({})", arg_list);

        println!("{}", arg_list);
        arg_types.push(parse_str(arg_list.as_str()).unwrap())

    };

    let function_return_type: Vec<TokenStream> = functions
        .iter()
        .map(|f| TokenStream::from_str(&f.return_type).unwrap())
        .collect();

    let distributable_enum = quote::quote! {
        #[derive(serde::Serialize, serde::Deserialize)]
        enum Distributable {
            #(#function_names #arg_types ),*
        }

        #[derive(serde::Serialize, serde::Deserialize)]
        enum ReverseDistributable {
            #(#function_names(#function_return_type)),*
        }
    };

    println!("{}", distributable_enum.to_token_stream().to_string());

    build_redirect_function(&functions).to_tokens(&mut tk);
    distributable_enum.to_tokens(&mut tk);
    tk.into()
}

fn build_redirect_function(functions: &Vec<DistributableFunction>) -> TokenStream {
    let function_definitions: Vec<ItemFn> = functions
        .iter()
        .map(|f| parse_str(f.raw.as_str()).unwrap())
        .collect();

    let mut arg_definitions: Vec<TokenStream> = vec![];
    let mut arg_names: Vec<Expr> = vec![];
    for function in functions.iter() {
        let arg_list: String = function
            .arguments
            .iter()
            .map(|arg| arg.0.clone())
            .collect::<Vec<String>>()
            .join(",");

        let arg_definition: String = function
            .arguments
            .iter()
            .map(|arg| format!("{}:{}", arg.0.clone(), arg.1.clone()))
            .collect::<Vec<String>>()
            .join(",");

        let arg_list = format!("({})", arg_list);
        let arg_definition = format!("({})", arg_definition);

        arg_names.push(parse_str(arg_list.as_str()).unwrap());
        arg_definitions.push(TokenStream::from_str(arg_definition.as_str()).unwrap());
    };


    let wrapper_function_name_list: Vec<Ident> = functions
        .iter()
        .map(|f| Ident::new(format!("internal_{}", f.name).as_str(), Span::call_site()))
        .collect();


    let function_name_list: Vec<Ident> = functions
        .iter()
        .map(|f| Ident::new(f.name.as_str(), Span::call_site()))
        .collect();

    let function_names_pascal: Vec<Ident> = functions
        .iter()
        .map(|f| Ident::new(&f.name.to_case(Case::Pascal), Span::call_site()))
        .collect();

    let enum_arms = quote::quote! {
        #( Distributable::#function_names_pascal #arg_names => #wrapper_function_name_list #arg_names),*
    };

    // We also need a reverse enum to match the return type to the function name

    let middleware_part = build_middleware_function();
    let function = quote::quote! {
        fn redirect_to_function(d: Distributable, function_name: String) ->ReverseDistributable {
            #(
                fn #wrapper_function_name_list #arg_definitions -> ReverseDistributable {
                    #function_definitions

                    // Calling the inner function here
                    let val = ReverseDistributable::#function_names_pascal(#function_name_list #arg_names);
                    return val;
                }
            );*

            #middleware_part

            match d {
                #enum_arms
            }
        }
    };

    println!("{}", function.to_token_stream().to_string());

    function.into()
}

fn build_middleware_function() -> TokenStream {
    let functions = MIDDLEWARE.lock().unwrap();

    let function_definitions: Vec<ItemFn> = functions
        .iter()
        .map(|f| parse_str(f.raw.as_str()).unwrap())
        .collect();

    let function_names: Vec<Ident> = functions
        .iter()
        .map(|f| Ident::new(f.name.as_str(), Span::call_site()))
        .collect();

    let call_tupple = quote::quote! { (&d, function_name); };


    quote::quote! {
        #(#function_definitions);*
        #(#function_names #call_tupple);*
    }

}

#[proc_macro]
pub fn build_test(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut tk = proc_macro2::TokenStream::new();

    let fn_1 = quote::quote! {
        fn a() {
            println!("This is printing something...");
        }
        a();
    };

    let fn_2 = quote::quote! {
        fn b() {}
    };

    let function_definitions = vec![fn_1, fn_2];


    let function = quote::quote! {
        fn redirect_to_function_t() {
            #(#function_definitions)*
        }
    };

    println!("{:?}", function.to_string());

    tk.extend(function);
    tk.into()
}
