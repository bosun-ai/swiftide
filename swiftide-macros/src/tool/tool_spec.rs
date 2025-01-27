use proc_macro2::TokenStream;
use quote::quote;

use super::{Description, ToolArgs};

pub fn tool_spec(tool_name: &str, args: &ToolArgs) -> TokenStream {
    let description = match &args.description {
        Description::Literal(description) => quote! { #description },
        Description::Path(path) => quote! { #path },
    };

    if args.param.is_empty() {
        quote! { swiftide::chat_completion::ToolSpec::builder().name(#tool_name).description(#description).build().unwrap() }
    } else {
        let params = args
            .param
            .iter()
            .map(|param| {
                let name = &param.name;
                let description = &param.description;

                quote! {
                    swiftide::chat_completion::ParamSpec::builder()
                        .name(#name)
                        .description(#description)
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
