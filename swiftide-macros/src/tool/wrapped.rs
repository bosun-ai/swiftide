use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, ItemFn};

pub(crate) fn struct_name(input: &ItemFn) -> Ident {
    let struct_name_str = input
        .sig
        .ident
        .to_string()
        .split('_') // Split by underscores
        .map(|s| {
            let mut chars = s.chars();
            chars
                .next()
                .map(|c| c.to_ascii_uppercase())
                .into_iter()
                .collect::<String>()
                + chars.as_str()
        })
        .collect::<String>();
    Ident::new(&struct_name_str, input.sig.ident.span())
}

pub(crate) fn wrap_tool_fn(input: &ItemFn) -> TokenStream {
    let fn_name = &input.sig.ident;
    let fn_args = &input.sig.inputs;
    let fn_body = &input.block;
    let fn_output = &input.sig.output;

    let struct_name = struct_name(input);

    let fn_args = fn_args.iter();

    quote! {
        #[derive(Clone, Default)]
        pub struct #struct_name {}

        pub fn #fn_name() -> Box<dyn ::swiftide::chat_completion::Tool> {
            Box::new(#struct_name {}) as Box<dyn ::swiftide::chat_completion::Tool>
        }

        impl #struct_name {
            pub async fn #fn_name(&self, #(#fn_args),*) #fn_output #fn_body
        }

    }
}

#[cfg(test)]
mod tests {
    use crate::assert_ts_eq;

    use super::*;
    use quote::quote;
    use syn::{parse_quote, ItemFn};

    #[test]
    fn test_wrap_tool_fn() {
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &dyn swiftide::traits::AgentContext, code_query: &str) -> std::result::Result<swiftide::chat_completion::ToolOutput, ::swiftide::chat_completion::errors::ToolError> {
                return Ok("hello".into())
            }
        };

        let output = wrap_tool_fn(&input);

        let expected = quote! {
            #[derive(Clone, Default)]
            pub struct SearchCode {}

            pub fn search_code() -> Box<dyn ::swiftide::chat_completion::Tool> {
                Box::new(SearchCode {}) as Box<dyn ::swiftide::chat_completion::Tool>
            }

            impl SearchCode {
                pub async fn search_code(&self, context: &dyn swiftide::traits::AgentContext, code_query: &str) -> std::result::Result<swiftide::chat_completion::ToolOutput, ::swiftide::chat_completion::errors::ToolError> {
                    return Ok("hello".into())
                }

            }
        };

        assert_ts_eq!(&output, &expected);
    }

    #[test]
    fn test_wrap_multiple_args() {
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &dyn swiftide::traits::AgentContext, code_query: &str, other_arg: &str) -> std::result::Result<swiftide::chat_completion::ToolOutput, ::swiftide::chat_completion::errors::ToolError> {
                return Ok("hello".into())
            }
        };

        let output = wrap_tool_fn(&input);

        let expected = quote! {
            #[derive(Clone, Default)]
            pub struct SearchCode {}

            pub fn search_code() -> Box<dyn ::swiftide::chat_completion::Tool> {
                Box::new(SearchCode {}) as Box<dyn ::swiftide::chat_completion::Tool>
            }

            impl SearchCode {
                pub async fn search_code(&self, context: &dyn swiftide::traits::AgentContext, code_query: &str, other_arg: &str) -> std::result::Result<swiftide::chat_completion::ToolOutput, ::swiftide::chat_completion::errors::ToolError> {
                    return Ok("hello".into())
                }

            }
        };

        assert_ts_eq!(&output, &expected);
    }
}
