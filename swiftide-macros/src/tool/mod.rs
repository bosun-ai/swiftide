use convert_case::{Case, Casing as _};
use darling::{ast::NestedMeta, Error, FromDeriveInput, FromMeta};
use proc_macro2::TokenStream;
use quote::quote;
use serde::ser::SerializeMap as _;
use syn::{spanned::Spanned, DeriveInput, FnArg, ItemFn, Pat, PatType};

mod args;
mod tool_spec;
mod wrapped;

#[derive(FromMeta, Default, Debug)]
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
pub(crate) fn tool_impl(input_args: &TokenStream, input: &ItemFn) -> TokenStream {
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

    let tool_spec = tool_spec::tool_spec(&tool_name, &args);

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
            if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
                if let Pat::Ident(ident) = &**pat {
                    seen_arg_names.push(ident.ident.to_string());

                    // If the argument is a reference, we need to referance the quote as well
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
            else { return Err(::swiftide::chat_completion::errors::ToolError::MissingArguments(format!("No arguments provided for {}", #tool_name))) };

            let args: #args_struct = ::swiftide::reexports::serde_json::from_str(&args)?;
            return self.#fn_name(agent_context, #(#arg_names),*).await;
        }
    };

    quote! {
        #tool_args

        #wrapped_fn

        #[::swiftide::reexports::async_trait::async_trait]
        impl ::swiftide::chat_completion::Tool for #tool_struct {
            async fn invoke(&self, agent_context: &dyn ::swiftide::traits::AgentContext, raw_args: Option<&str>) -> ::std::result::Result<::swiftide::chat_completion::ToolOutput, ::swiftide::chat_completion::errors::ToolError> {
                #invoke_body
            }

            fn name(&self) -> &'static str {
                #tool_name
            }

            fn tool_spec(&self) -> ::swiftide::chat_completion::ToolSpec {
                #tool_spec
            }
        }
    }
}

#[derive(FromDeriveInput)]
#[darling(attributes(tool), supports(struct_named))]
struct ToolDerive {
    ident: syn::Ident,
    #[allow(dead_code)]
    attrs: Vec<syn::Attribute>,
    #[darling(flatten)]
    tool: ToolArgs,
}

pub(crate) fn tool_derive_impl(input: &DeriveInput) -> syn::Result<TokenStream> {
    let parsed: ToolDerive = ToolDerive::from_derive_input(input)?;
    let struct_ident = &parsed.ident;

    // Build the args struct
    let args_struct_name = syn::Ident::new(&format!("{struct_ident}Args"), struct_ident.span());
    let args_struct_fields = parsed
        .tool
        .param
        .iter()
        .map(|p| {
            let field_name = syn::Ident::new(&p.name, struct_ident.span());
            quote! { pub #field_name: String }
        })
        .collect::<Vec<_>>();

    let tool_args = if args_struct_fields.is_empty() {
        quote! {}
    } else {
        quote! {
            #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize, Debug)]
            pub struct #args_struct_name {
                #(#args_struct_fields),*
            }
        }
    };

    let arg_names = parsed
        .tool
        .param
        .iter()
        .map(|param| {
            let field_name = syn::Ident::new(&param.name, struct_ident.span());
            quote! { args.#field_name}
        })
        .collect::<Vec<_>>();

    // Build the trait impl
    let expected_fn_name = struct_ident.to_string().to_case(Case::Snake);
    let expected_fn_ident = syn::Ident::new(&expected_fn_name, struct_ident.span());
    let invoke_body = if arg_names.is_empty() {
        quote! { return self.#expected_fn_ident(agent_context).await }
    } else {
        quote! {
            let Some(args) = raw_args
            else { return Err(::swiftide::chat_completion::errors::ToolError::MissingArguments(format!("No arguments provided for {}", #expected_fn_name))) };

            let args: #args_struct_name = ::swiftide::reexports::serde_json::from_str(&args)?;
            return self.#expected_fn_ident(agent_context, #(&#arg_names),*).await;
        }
    };

    let tool_spec = tool_spec::tool_spec(&expected_fn_name, &parsed.tool);

    let struct_lifetimes = input
        .generics
        .lifetimes()
        .map(|l| &l.lifetime)
        .collect::<Vec<_>>();

    let struct_lifetime = if struct_lifetimes.is_empty() {
        quote! {}
    } else {
        quote! { <#(#struct_lifetimes),*> }
    };

    Ok(quote! {
        #tool_args


        #[async_trait::async_trait]
        impl #struct_lifetime swiftide::chat_completion::Tool for #struct_ident #struct_lifetime {
            async fn invoke(&self, agent_context: &dyn swiftide::traits::AgentContext, raw_args: Option<&str>) -> std::result::Result<swiftide::chat_completion::ToolOutput, ::swiftide::chat_completion::errors::ToolError> {
                #invoke_body
            }

            fn name(&self) -> &'static str {
                #expected_fn_name
            }

            fn tool_spec(&self) -> swiftide::chat_completion::ToolSpec {
                #tool_spec
            }
        }
    })
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
            pub async fn search_code(context: &dyn AgentContext, code_query: &str) -> Result<ToolOutput, ToolError> {
                return Ok("hello".into())
            }
        };

        let output = tool_impl(&args, &input);

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

        let output = tool_impl(&args, &input);

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
}
