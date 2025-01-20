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

fn classify_type_path(type_path: &syn::TypePath) -> &'static str {
    // The last segment should hold the actual type name, e.g. "String" or "Vec".
    // (Ignore multi-segment paths like `std::collections::HashMap` by just checking
    // the final segment.)
    let segment = if let Some(seg) = type_path.path.segments.last() {
        seg
    } else {
        return "String";
    };

    let ident_str = segment.ident.to_string();

    // If it’s an actual generic like Vec<T>, parse out the identifier and check it.
    match ident_str.as_str() {
        // Known string type:
        "String" => "String",

        // Common numeric primitives:
        "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize" | "f32"
        | "f64" => "Number",

        // A generic known container: check if it’s `Vec<...>`
        "Vec" => "Array",

        // Could handle more explicitly, e.g. "HashMap" → "Object" or something else

        // Fallback for any other type:
        _ => "String",
    }
}
