use proc_macro2::TokenStream;
use quote::quote;

use super::args::{Description, ToolArgs};

pub fn tool_spec(args: &ToolArgs) -> TokenStream {
    let tool_name = args.tool_name();
    let description = match &args.tool_description() {
        Description::Literal(description) => quote! { #description },
        Description::Path(path) => quote! { #path },
    };

    let builder = quote! {
        swiftide::chat_completion::ToolSpec::builder()
            .name(#tool_name)
            .description(#description)
    };

    if args.tool_params().is_empty() {
        quote! { #builder.build().unwrap() }
    } else {
        let args_struct_ident = args.args_struct_ident();
        quote! {
            #builder
                .parameters_schema(::swiftide::reexports::schemars::schema_for!(#args_struct_ident))
                .build()
                .unwrap()
        }
    }
}
