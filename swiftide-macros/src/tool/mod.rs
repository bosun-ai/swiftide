use darling::{ast::NestedMeta, Error, FromMeta};
use proc_macro2::TokenStream;
use quote::quote;
use serde::ser::SerializeMap as _;
use syn::{spanned::Spanned, FnArg, ItemFn, Pat, PatType};

mod args;
mod json_spec;
mod wrapped;

#[derive(FromMeta, Default)]
#[darling(default)]
struct ToolArgs {
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

impl serde::Serialize for ParamOptions {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(
            &self.name,
            &serde_json::json!({
                "type": "string",
                "description": self.description
            }),
        )?;
        map.end()
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn tool_impl(input_args: TokenStream, input: &ItemFn) -> TokenStream {
    let args = match parse_args(input_args.clone()) {
        Ok(args) => args,
        Err(e) => return e.write_errors(),
    };
    let fn_name = &input.sig.ident;
    let fn_args = &input.sig.inputs;
    let tool_name = fn_name.to_string();

    let tool_args = args::build_tool_args(input).unwrap_or_else(syn::Error::into_compile_error);
    let args_struct = args::args_struct_name(input);
    let tool_struct = wrapped::struct_name(input);

    let wrapped_fn = wrapped::wrap_tool_fn(input);

    let json_spec = json_spec::json_spec(&tool_name, &args);

    let mut found_spec_arg_names = args
        .param
        .iter()
        .map(|param| param.name.clone())
        .collect::<Vec<_>>();
    found_spec_arg_names.sort();

    let mut seen_arg_names = vec![];

    let arg_names = fn_args
        .iter()
        .skip(1)
        .filter_map(|arg| {
            if let FnArg::Typed(PatType { pat, .. }) = arg {
                if let Pat::Ident(ident) = &**pat {
                    seen_arg_names.push(ident.ident.to_string());

                    Some(quote! { args.#ident })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    seen_arg_names.sort();

    if found_spec_arg_names != seen_arg_names {
        let missing_args = found_spec_arg_names
            .iter()
            .filter(|name| !seen_arg_names.contains(name))
            .collect::<Vec<_>>();

        let missing_params = seen_arg_names
            .iter()
            .filter(|name| !found_spec_arg_names.contains(name))
            .collect::<Vec<_>>();

        let mut messages = vec![];
        if !missing_args.is_empty() {
            messages.push(format!(
                "The following parameters are missing from the function signature: {missing_args:?}"
            ));
        }

        if !missing_params.is_empty() {
            messages.push(format!(
                "The following parameters are missing from the spec: {missing_params:?}"
            ));
        }

        return syn::Error::new(
            input_args.span(),
            format!(
                "Arguments in spec and in function do not match:\n {}",
                messages.join(", ")
            ),
        )
        .into_compile_error();
    }

    let invoke_body = if arg_names.is_empty() {
        quote! {
            return self.#fn_name(agent_context).await;
        }
    } else {
        quote! {
            let Some(args) = raw_args
            else { hidden::bail!("No arguments provided for {}", #tool_name) };

            let args: #args_struct = serde_json::from_str(&args)?;
            return self.#fn_name(agent_context, #(#arg_names).*).await;
        }
    };

    let imports = quote! {
            pub use ::anyhow::{bail, Result};
            pub use ::swiftide_core::chat_completion::{JsonSpec, ToolOutput };
            pub use ::swiftide_core::{Tool, AgentContext};
            pub use ::async_trait::async_trait;
    };

    quote! {
        mod hidden {
            #imports
        }

        #tool_args

        #wrapped_fn

        #[hidden::async_trait]
        impl hidden::Tool for #tool_struct {
            // TODO: Handle no arguments
            async fn invoke(&self, agent_context: &dyn hidden::AgentContext, raw_args: Option<&str>) -> hidden::Result<hidden::ToolOutput> {
                #invoke_body
            }

            fn name(&self) -> &'static str {
                #tool_name
            }

            fn json_spec(&self) -> hidden::JsonSpec {
                #json_spec
            }
        }
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
    fn test_snapshot_single_arg() {
        let args = quote! {
            description = "Hello world tool",
            param(
                name = "code_query",
                description = "my param description"
            )
        };
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &dyn AgentContext, code_query: &str) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = tool_impl(args, &input);

        insta::assert_snapshot!(crate::test_utils::pretty_macro_output(&output));
    }
}
