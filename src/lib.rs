#![feature(proc_macro_quote)]
extern crate proc_macro;
use quote::format_ident;
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};
use syn::{FnArg, ItemFn, Pat, __private::ToTokens, parse_str, Expr, ReturnType};

use convert_case::{Case, Casing};
use lazy_static::lazy_static;
use proc_macro2::{Ident, Span, TokenStream};

lazy_static! {
    static ref FUNCTIONS: Arc<Mutex<Vec<DistributableFunction>>> = Arc::new(Mutex::new(vec![]));
    static ref MIDDLEWARE: Arc<Mutex<Vec<DistributableFunction>>> = Arc::new(Mutex::new(vec![]));
}

struct DistributableFunction {
    name: String,
    arguments: Vec<(String, String)>,
    raw: String,
    return_type: String,
}

impl DistributableFunction {
    fn parse(stream: &proc_macro::TokenStream) -> ItemFn {
        let item: ItemFn = parse_str(&stream.to_string()).unwrap();
        item
    }

    fn new(stream: &proc_macro::TokenStream) -> Self {
        let mut arguments = vec![];
        let item = Self::parse(stream);

        for i in item.sig.inputs.iter() {
            if let FnArg::Typed(t) = i {
                if let Pat::Ident(named_arg) = &*t.pat {
                    arguments.push((
                        named_arg.ident.to_string(),
                        t.ty.to_token_stream().to_string(),
                    ));
                }
            };
        }

        let return_quote = match &item.sig.output {
            ReturnType::Default => quote::quote!(()),
            ReturnType::Type(_, b) => quote::quote!(#b),
        };

        DistributableFunction {
            name: item.sig.ident.to_string(),
            arguments,
            raw: item.to_token_stream().to_string(),
            return_type: return_quote.to_token_stream().to_string(),
        }
    }

    fn get_name(&self) -> Ident {
        format_ident!("{}", self.name)
    }

    fn get_pascal_name(&self) -> Ident {
        format_ident!("{}", self.name.to_case(Case::Pascal))
    }

    fn get_arg_types(&self) -> Expr {
        let arg_list: String = self 
            .arguments
            .iter()
            .map(|arg| arg.1.clone())
            .collect::<Vec<String>>()
            .join(",");

        println!("{}", arg_list);


        parse_str(&format!("({})", arg_list)).unwrap()
    }

    fn get_arg_names(&self) -> Expr {
        let arg_names: String = self 
            .arguments
            .iter()
            .map(|arg| arg.0.clone())
            .collect::<Vec<String>>()
            .join(",");


        parse_str(&format!("({})", arg_names)).unwrap()
    }

    fn get_return_type(&self) -> TokenStream {
        TokenStream::from_str(&self.return_type).unwrap()
    }

    fn get_item_fn(&self) -> ItemFn {
        parse_str(self.raw.as_str()).unwrap()
    }

    fn get_full_arg_sig(&self) -> TokenStream {
        let arg_definition: String = self 
            .arguments
            .iter()
            .map(|arg| format!("{}:{}", arg.0.clone(), arg.1.clone()))
            .collect::<Vec<String>>()
            .join(",");

        let arg_definition = format!("({})", arg_definition);

        TokenStream::from_str(arg_definition.as_str()).unwrap()
    }


}

#[proc_macro_attribute]
pub fn middleware(
    _here: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let distributable_function = DistributableFunction::new(&item);
    MIDDLEWARE.lock().unwrap().push(distributable_function);

    let item = DistributableFunction::parse(&item);

    quote::quote! {
        #[allow(dead_code)]
        #item
    }
    .into()
}

#[proc_macro_attribute]
pub fn distributable(
    _here: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let mut lock = FUNCTIONS.lock().unwrap();
    let dt = DistributableFunction::new(&item);

    let name = dt.get_name();
    let name_string: String = dt.name.clone();
    let pascal_name = dt.get_pascal_name();
    let return_type = dt.get_return_type();
    let full_arg_sig = dt.get_full_arg_sig();
    let call_arg = dt.get_arg_names();

    let quote = quote::quote! {
        // #[allow(dead_code)]
        // #item
        fn #name #full_arg_sig -> #return_type {
            let variant = redirect_to_function(
                Distributable::#pascal_name #call_arg,
                #name_string
            );
            if let ReverseDistributable::#pascal_name(val) = variant {
                return val
            } else {
                panic!("Macro won't ever reach here.");
            }
        }


    };
    println!("{}", quote.to_token_stream().to_string());

    lock.push(dt);

    quote.into()
}

#[proc_macro]
pub fn build(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let functions = FUNCTIONS.lock().unwrap();
    let mut tk = TokenStream::new();

    let function_names: Vec<Ident> = functions
        .iter()
        .map(|function| function.get_pascal_name())
        .collect();

    let arg_types : Vec<Expr> = functions
        .iter()
        .map(|f| f.get_arg_types())
        .collect();

    let function_return_type: Vec<TokenStream> = functions
        .iter()
        .map(|f| f.get_return_type())
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

fn build_redirect_function(functions: &[DistributableFunction]) -> TokenStream {
    let function_definitions: Vec<ItemFn> = functions
        .iter()
        .map(|f| f.get_item_fn()) 
        .collect();

    let arg_definitions: Vec<TokenStream> = functions
        .iter()
        .map(|f| f.get_full_arg_sig())
        .collect();

    let arg_names: Vec<Expr> = functions
        .iter()
        .map(|f| f.get_arg_names())
        .collect();

    // TODO: Combine these three transformations of each function as properties on a struct
    // This way only one loop is needed, and the association between the three can be more clear.
    let wrapper_function_name_list: Vec<Ident> = functions
        .iter()
        .map(|f| Ident::new(&format!("internal_{}", f.name), Span::call_site()))
        .collect();

    let function_name_list: Vec<Ident> = functions
        .iter()
        .map(|f| f.get_name())
        .collect();

    let function_names_pascal: Vec<Ident> = functions
        .iter()
        .map(|f| f.get_pascal_name())
        .collect();

    let enum_arms = quote::quote! {
        #( Distributable::#function_names_pascal #arg_names => #wrapper_function_name_list #arg_names),*
    };

    // We also need a reverse enum to match the return type to the function name

    let middleware_part = build_middleware_function();
    let function = quote::quote! {
        fn redirect_to_function(d: Distributable, function_name: &str) ->ReverseDistributable {
            #(
                fn #wrapper_function_name_list #arg_definitions -> ReverseDistributable {
                    #function_definitions

                    // Calling the inner function here
                    let val = ReverseDistributable::#function_names_pascal(#function_name_list #arg_names);
                    return val;
                }
            );*

            println!("[RedirectToFunction] function {}", &function_name);

            #middleware_part

            let result = match d {
                #enum_arms
            };

            println!("[RedirectToFunction] Returning from {}", &function_name);

            return result
        }
    };

    println!("{}", function.to_token_stream().to_string());

    function
}

fn build_middleware_function() -> TokenStream {
    let functions = MIDDLEWARE.lock().unwrap();

    let function_definitions: Vec<ItemFn> = functions
        .iter()
        .map(|f| parse_str(&f.raw).unwrap())
        .collect();

    let function_names: Vec<Ident> = functions
        .iter()
        .map(|f| Ident::new(&f.name, Span::call_site()))
        .collect();

    let call_tupple = quote::quote! { (&d, &function_name); };

    quote::quote! {
        #(#function_definitions);*
        #(#function_names #call_tupple);*
    }
}
