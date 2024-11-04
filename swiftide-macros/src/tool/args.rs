use proc_macro2::TokenStream;
use quote::{quote, ToTokens as _};
use syn::{
    parse::Result, token::Pub, Error, Field, Fields, FnArg, Ident, ItemFn, ItemStruct, Lifetime,
    PatType, Token, Type, TypeReference,
};

pub(crate) fn build_tool_args(input: &ItemFn) -> Result<TokenStream> {
    validate_first_argument_is_agent_context(input)?;

    let args = &input.sig.inputs;
    let mut struct_fields = Vec::new();

    let mut has_lifetime = false;

    for arg in args.iter().skip(1) {
        if let syn::FnArg::Typed(PatType { pat, ty, .. }) = arg {
            if let syn::Pat::Ident(ident) = &**pat {
                // Check if the type is a reference and needs a lifetime
                if let Type::Reference(TypeReference { lifetime, elem, .. }) = &**ty {
                    // Add a lifetime if it’s specified; otherwise, use `'a` if `has_lifetime` is true
                    has_lifetime = true;

                    let lifetime: Lifetime = syn::parse_str("'a").unwrap();
                    struct_fields.push(quote! { #ident: &#lifetime #elem });
                } else {
                    // If no reference type, just use the type as-is
                    struct_fields.push(quote! { #ident: #ty });
                }
            }
        }
    }

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
        .collect::<String>()
        + "Args";
    let struct_name = Ident::new(&struct_name_str, input.sig.ident.span());

    if has_lifetime {
        Ok(quote! {
            struct #struct_name<'a> {
                pub #(#struct_fields),*
            }
        })
    } else {
        Ok(quote! {
            struct #struct_name {
                pub #(#struct_fields),*
            }
        })
    }
}

fn validate_first_argument_is_agent_context(input_fn: &ItemFn) -> Result<()> {
    // let first_arg = input_fn.sig.inputs.first();
    let expected_first_arg = quote! { &impl AgentContext };
    let error_msg = "The first argument must be `&impl AgentContext`";

    if let Some(FnArg::Typed(first_arg)) = input_fn.sig.inputs.first() {
        if first_arg.ty.to_token_stream().to_string() != expected_first_arg.to_string() {
            return Err(Error::new_spanned(&first_arg.ty, error_msg));
        }
    } else {
        return Err(Error::new_spanned(&input_fn.sig, error_msg));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::assert_ts_eq;

    use super::*;
    use quote::quote;
    use syn::{parse_quote, ItemFn};

    #[test]
    fn test_agent_context_as_first_arg_required() {
        let input: ItemFn = parse_quote! {
            pub async fn search_code(code_query: &str) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = build_tool_args(&input).unwrap_err();

        assert_eq!(
            output.to_string(),
            "The first argument must be `&impl AgentContext`"
        );
    }

    #[test]
    fn test_simple_tool() {
        let input: ItemFn = parse_quote! {
            pub async fn search_code(context: &impl AgentContext, code_query: &str) -> Result<ToolOutput> {
                return Ok("hello".into())
            }
        };

        let output = build_tool_args(&input).unwrap();

        let expected = quote! {
            struct SearchCodeArgs<'a> {
                pub code_query: &'a str,
            }
        };

        assert_ts_eq!(&output, &expected);
    }
}
