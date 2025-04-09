#![allow(clippy::used_underscore_binding)]

use args::ToolArgs;
use darling::{Error, FromDeriveInput};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_quote, DeriveInput, FnArg, ItemFn, Pat, PatType};

mod args;
mod rust_to_json_type;
mod tool_spec;
mod wrapped;

#[allow(clippy::too_many_lines)]
pub(crate) fn tool_attribute_impl(input_args: &TokenStream, input: &ItemFn) -> TokenStream {
    let tool_args = match ToolArgs::try_from_attribute_input(input, input_args.clone()) {
        Ok(args) => args,
        Err(e) => return e.write_errors(),
    };

    let fn_name = &input.sig.ident;

    let args_struct = tool_args.args_struct();
    let args_struct_ident = tool_args.args_struct_ident();
    let arg_names = input
        .sig
        .inputs
        .iter()
        .skip(1)
        .filter_map(|arg| {
            if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
                if let Pat::Ident(ident) = &**pat {
                    // If the argument is a reference, we need to reference the quote as well
                    if let syn::Type::Reference(_) = &**ty {
                        Some(quote! { &args.#ident })
                    } else {
                        Some(quote! { args.#ident })
                    }
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let tool_name = tool_args.tool_name();

    let tool_struct = wrapped::struct_name(input);

    let wrapped_fn = wrapped::wrap_tool_fn(input);

    let tool_spec = tool_spec::tool_spec(&tool_args);

    let invoke_body = if arg_names.is_empty() {
        quote! {
            return self.#fn_name(agent_context).await;
        }
    } else {
        quote! {
            let Some(args) = raw_args
            else { return Err(::swiftide::chat_completion::errors::ToolError::MissingArguments(format!("No arguments provided for {}", #tool_name))) };

            let args: #args_struct_ident = ::swiftide::reexports::serde_json::from_str(&args)?;
            return self.#fn_name(agent_context, #(#arg_names),*).await;
        }
    };

    let boxed_from = boxed_from(&tool_struct, &parse_quote!());

    quote! {
        #args_struct

        #wrapped_fn

        #[::swiftide::reexports::async_trait::async_trait]
        impl ::swiftide::chat_completion::Tool for #tool_struct {
            async fn invoke(&self, agent_context: &dyn ::swiftide::traits::AgentContext, raw_args: Option<&str>) -> ::std::result::Result<::swiftide::chat_completion::ToolOutput, ::swiftide::chat_completion::errors::ToolError> {
                #invoke_body
            }

            fn name<'TOOL>(&'TOOL self) -> std::borrow::Cow<'TOOL, str> {
                #tool_name.into()
            }

            fn tool_spec(&self) -> ::swiftide::chat_completion::ToolSpec {
                #tool_spec
            }
        }

        #boxed_from
    }
}

#[derive(FromDeriveInput)]
#[darling(attributes(tool), supports(struct_any), and_then = ToolDerive::update_defaults, forward_attrs(allow, doc, cfg))]
struct ToolDerive {
    ident: syn::Ident,
    #[allow(dead_code)]
    attrs: Vec<syn::Attribute>,
    #[darling(flatten)]
    tool: ToolArgs,
}

impl ToolDerive {
    pub fn update_defaults(mut self) -> Result<Self, Error> {
        self.tool.with_name_from_ident(&self.ident);
        self.tool.infer_param_types()?;
        Ok(self)
    }
}

pub(crate) fn tool_derive_impl(input: &DeriveInput) -> syn::Result<TokenStream> {
    let parsed: ToolDerive = ToolDerive::from_derive_input(input)?;
    let struct_ident = &parsed.ident;

    let expected_fn_name = parsed.tool.fn_name();
    let expected_fn_ident = syn::Ident::new(expected_fn_name, struct_ident.span());

    let invoke_tool_args = parsed
        .tool
        .arg_names()
        .into_iter()
        .map(|name| {
            let name = syn::Ident::new(name, struct_ident.span());
            quote! { args.#name }
        })
        .collect::<Vec<_>>();
    let args_struct_ident = parsed.tool.args_struct_ident();
    let args_struct = parsed.tool.args_struct();

    let invoke_body = if invoke_tool_args.is_empty() {
        quote! { return self.#expected_fn_ident(agent_context).await }
    } else {
        quote! {
            let Some(args) = raw_args
            else { return Err(::swiftide::chat_completion::errors::ToolError::MissingArguments(format!("No arguments provided for {}", #expected_fn_name))) };

            let args: #args_struct_ident = ::swiftide::reexports::serde_json::from_str(&args)?;
            return self.#expected_fn_ident(agent_context, #(&#invoke_tool_args),*).await;
        }
    };

    let tool_spec = tool_spec::tool_spec(&parsed.tool);

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Arg should be, if empty None, else Some(&args)
    let boxed_from = boxed_from(struct_ident, &input.generics);
    Ok(quote! {
        #args_struct


        #[async_trait::async_trait]
        impl #impl_generics swiftide::chat_completion::Tool for #struct_ident #ty_generics #where_clause {
            async fn invoke(&self, agent_context: &dyn swiftide::traits::AgentContext, raw_args: Option<&str>) -> std::result::Result<swiftide::chat_completion::ToolOutput, ::swiftide::chat_completion::errors::ToolError> {
                #invoke_body
            }

            fn name<'TOOL>(&'TOOL self) -> std::borrow::Cow<'TOOL, str> {
                #expected_fn_name.into()
            }

            fn tool_spec(&self) -> swiftide::chat_completion::ToolSpec {
                #tool_spec
            }
        }

        #boxed_from
    })
}

fn boxed_from(struct_ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    if !generics.params.is_empty() {
        return quote!();
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let lt_ident = if let Some(other_lifetime) = generics.lifetimes().next() {
        let lifetime = &other_lifetime.lifetime;
        quote!(+ #lifetime)
    } else {
        quote!()
    };

    quote! {
        impl #impl_generics From<#struct_ident #ty_generics> for Box<dyn ::swiftide::chat_completion::Tool #lt_ident> #where_clause {
            fn from(val: #struct_ident) -> Self {
                Box::new(val) as Box<dyn ::swiftide::chat_completion::Tool>
            }
        }
    }
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
            pub async fn search_code(context: &dyn AgentContext, code_query: &str) -> Result<ToolOutput, ToolError> {
                return Ok("hello".into())
            }
        };

        let output = tool_attribute_impl(&args, &input);

        insta::assert_snapshot!(crate::test_utils::pretty_macro_output(&output));
    }

    #[test]
    fn test_snapshot_single_arg_option() {
        let args = quote! {
            description = "Hello world tool",
            param(
                name = "code_query",
                description = "my param description"
            )
        };
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &dyn AgentContext, code_query: &Option<String>) -> Result<ToolOutput, ToolError> {
                return Ok("hello".into())
            }
        };

        let output = tool_attribute_impl(&args, &input);

        insta::assert_snapshot!(crate::test_utils::pretty_macro_output(&output));
    }

    #[test]
    fn test_snapshot_multiple_args() {
        let args = quote! {
            description = "Hello world tool",
            param(
                name = "code_query",
                description = "my param description"
            ),
            param(
                name = "other",
                description = "my param description"
            )
        };
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &dyn AgentContext, code_query: &str, other: &str) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = tool_attribute_impl(&args, &input);

        insta::assert_snapshot!(crate::test_utils::pretty_macro_output(&output));
    }

    #[test]
    fn test_snapshot_derive() {
        let input: DeriveInput = parse_quote! {
            #[tool(description="Hello derive")]
            pub struct HelloDerive {
                my_thing: String
            }
        };

        let output = tool_derive_impl(&input).unwrap();

        insta::assert_snapshot!(crate::test_utils::pretty_macro_output(&output));
    }

    #[test]
    fn test_snapshot_derive_with_args() {
        let input: DeriveInput = parse_quote! {
            #[tool(description="Hello derive", param(name="test", description="test param"))]
            pub struct HelloDerive {
                my_thing: String
            }
        };

        let output = tool_derive_impl(&input).unwrap();

        insta::assert_snapshot!(crate::test_utils::pretty_macro_output(&output));
    }

    #[test]
    fn test_snapshot_derive_with_option() {
        let input: DeriveInput = parse_quote! {
            #[tool(description="Hello derive", param(name="test", description="test param", required = false))]
            pub struct HelloDerive {
                my_thing: String
            }
        };

        let output = tool_derive_impl(&input).unwrap();

        insta::assert_snapshot!(crate::test_utils::pretty_macro_output(&output));
    }

    #[test]
    fn test_snapshot_derive_with_lifetime() {
        let input: DeriveInput = parse_quote! {
            #[tool(description="Hello derive", param(name="test", description="test param"))]
            pub struct HelloDerive<'a> {
                my_thing: &'a str,
            }
        };

        let output = tool_derive_impl(&input).unwrap();

        insta::assert_snapshot!(crate::test_utils::pretty_macro_output(&output));
    }

    #[test]
    fn test_snapshot_derive_with_generics() {
        let input: DeriveInput = parse_quote! {
            #[tool(description="Hello derive", param(name="test", description="test param"))]
            pub struct HelloDerive<S: Send + Sync + Clone> {
                my_thing: S,
            }
        };

        let output = tool_derive_impl(&input).unwrap();

        insta::assert_snapshot!(crate::test_utils::pretty_macro_output(&output));
    }
}
