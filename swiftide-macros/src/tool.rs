use darling::{ast::NestedMeta, Error, FromMeta};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, Ident, ItemFn, ItemStruct};

#[derive(FromMeta, Default)]
#[darling(default)]
struct ToolArgs {
    name: String,
    description: String,

    #[darling(multiple)]
    param: Vec<ParamOptions>,
}

#[derive(FromMeta, Debug, Default)]
#[darling(default)]
struct ParamOptions {
    name: String,
    description: String,
    // TODO: I.e. openai also supports enums instead of strings as arg type
}

#[allow(clippy::too_many_lines)]
pub(crate) fn tool_impl(args: TokenStream, input: ItemFn) -> TokenStream {
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(e) => return e.write_errors(),
    };

    let args = &input.sig.inputs;
    let mut struct_fields = Vec::new();

    for arg in args {
        if let syn::FnArg::Typed(pat_type) = arg {
            let ident = match &*pat_type.pat {
                syn::Pat::Ident(ident) => ident.ident.clone(),
                _ => panic!("Only simple identifiers are supported for now"),
            };
            struct_fields.push(quote! {
                #ident: #pat_type.ty,
            });
        }
    }

    /// Building the args struct
    quote! {
        struct HelloWorld {
            #(#struct_fields)*
        }
        // The args
        // new wrapper function that takes parsed args and calls old function
        // old function
        // Tool impl
    }
}

fn parse_args(args: TokenStream) -> Result<ToolArgs, Error> {
    let attr_args = NestedMeta::parse_meta_list(args)?;

    ToolArgs::from_list(&attr_args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use syn::{parse_quote, ItemFn};

    #[test]
    fn test_simple_tool() {
        let args = quote! {
            name = "Hello world",
            description = "Hello world tool",
            param(
                name = "name",
                description = "Your name"
            )
        };
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &impl AgentContext, code_query: &str) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = tool_impl(args.into(), input);

        let expected = quote! {
            struct HelloWorld {
            context: &impl AgentContext,
            code_query: &str,
            }
        };

        assert_eq!(output.to_string(), expected.to_string());
    }
}
