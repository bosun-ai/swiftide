use proc_macro2::TokenStream;
use quote::{quote, TokenStreamExt as _};

use crate::tool::ParamType;

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

    let ident = proc_macro2::Ident::new(&format!("{ty}"), proc_macro2::Span::call_site());

    quote! { ::swiftide::chat_completion::ParamType::#ident }
}

// fn classify_type_path(type_path: &syn::TypePath) -> &'static str {
//     // The last segment should hold the actual type name, e.g. "String" or "Vec".
//     // (Ignore multi-segment paths like `std::collections::HashMap` by just checking
//     // the final segment.)
//     let segment = if let Some(seg) = type_path.path.segments.last() {
//         seg
//     } else {
//         return "String";
//     };
//
//     let ident_str = segment.ident.to_string();
//
//     // If it’s an actual generic like Vec<T>, parse out the identifier and check it.
//     match ident_str.as_str() {
//         // Known string type:
//         "String" => "String",
//
//         // Common numeric primitives:
//         "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize" | "f32"
//         | "f64" => "Number",
//
//         // A generic known container: check if it’s `Vec<...>`
//         "Vec" => "Array",
//
//         // Could handle more explicitly, e.g. "HashMap" → "Object" or something else
//
//         // Fallback for any other type:
//         _ => "String",
//     }
// }
