use proc_macro2::TokenStream;
use quote::quote;

use crate::tool::args::ParamType;

use super::args::{Description, ToolArgs};

pub fn tool_spec(args: &ToolArgs) -> TokenStream {
    let tool_name = args.tool_name();
    let description = match &args.tool_description() {
        Description::Literal(description) => quote! { #description },
        Description::Path(path) => quote! { #path },
    };

    if args.tool_params().is_empty() {
        quote! { swiftide::chat_completion::ToolSpec::builder().name(#tool_name).description(#description).build().unwrap() }
    } else {
        let params = args
            .tool_params()
            .iter()
            .map(|param| {
                let name = &param.name;
                let description = &param.description;
                let ty = param_type_to_token_stream(param.json_type);

                quote! {
                    swiftide::chat_completion::ParamSpec::builder()
                        .name(#name)
                        .description(#description)
                        .ty(#ty)
                        .build().expect("infallible")

                }
            })
            .collect::<Vec<_>>();

        quote! {
            swiftide::chat_completion::ToolSpec::builder()
            .name(#tool_name)
            .description(#description)
            .parameters(vec![#(#params),*])
            .build()
            .unwrap()
        }
    }
}

fn param_type_to_token_stream(ty: ParamType) -> TokenStream {
    let ty = match ty {
        ParamType::String => "String",
        ParamType::Number => "Number",
        ParamType::Boolean => "Boolean",
        ParamType::Array => "Array",
    };

    let ident = proc_macro2::Ident::new(ty, proc_macro2::Span::call_site());

    quote! { ::swiftide::chat_completion::ParamType::#ident }
}
