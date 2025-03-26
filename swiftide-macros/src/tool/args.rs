use convert_case::{Case, Casing as _};
use darling::{ast::NestedMeta, Error, FromMeta};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens as _};
use syn::{parse_quote, FnArg, Ident, ItemFn, Pat, PatType};

use super::rust_to_json_type::rust_type_to_json_type;

#[derive(FromMeta, Default, Debug)]
pub struct ToolArgs {
    #[darling(default)]
    /// Name of the tool
    /// Defaults to the underscored version of the function name or struct
    name: String,

    /// Name of the function to call
    /// Defaults to the underscored version of the function name or struct
    #[darling(default)]
    fn_name: String,

    /// Description of the tool
    description: Description,

    /// Parameters the tool can take
    #[darling(multiple, rename = "param")]
    params: Vec<ParamOptions>,
}

#[derive(FromMeta, Debug)]
#[darling(default)]
pub struct ParamOptions {
    pub name: String,
    pub description: String,

    /// The type the parameter should be in the JSON spec
    /// Defaults to `String`
    pub json_type: ParamType,

    /// The type the parameter should be in Rust
    /// Defaults to what can be derived from `json_type`
    pub rust_type: syn::Type,
}

impl Default for ParamOptions {
    fn default() -> Self {
        ParamOptions {
            name: String::new(),
            description: String::new(),
            json_type: ParamType::String,
            rust_type: syn::parse_quote! { String },
        }
    }
}

impl ParamOptions {
    #[cfg(test)]
    pub fn new(name: &str, description: &str, json_type: ParamType, rust_type: syn::Type) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            json_type,
            rust_type,
        }
    }
}

#[derive(Debug, FromMeta, PartialEq, Eq, Default, Clone, Copy)]
#[darling(rename_all = "camelCase")]
pub enum ParamType {
    #[default]
    String,
    Number,
    Boolean,
    Array,
}

impl ParamType {
    fn default_rust_type(&self) -> syn::Type {
        match self {
            ParamType::String => syn::parse_quote! { String },
            ParamType::Number => syn::parse_quote! { usize },
            ParamType::Boolean => syn::parse_quote! { bool },
            ParamType::Array => syn::parse_quote! { Vec<String> },
        }
    }

    fn try_from_rust_type(ty: &syn::Type) -> Result<ParamType, Error> {
        rust_type_to_json_type(&ty)
    }
}

#[derive(Debug)]
pub enum Description {
    Literal(String),
    Path(syn::Path),
}

impl Default for Description {
    fn default() -> Self {
        Description::Literal(String::new())
    }
}

impl FromMeta for Description {
    fn from_expr(expr: &syn::Expr) -> darling::Result<Self> {
        match expr {
            syn::Expr::Lit(lit) => {
                if let syn::Lit::Str(s) = &lit.lit {
                    Ok(Description::Literal(s.value()))
                } else {
                    Err(Error::unsupported_format(
                        "expected a string literal or a const",
                    ))
                }
            }
            syn::Expr::Path(path) => Ok(Description::Path(path.path.clone())),
            _ => Err(Error::unsupported_format(
                "expected a string literal or a const",
            )),
        }
    }
}

impl ToolArgs {
    pub fn try_from_attribute_input(input: &ItemFn, args: TokenStream) -> Result<Self, Error> {
        validate_first_argument_is_agent_context(input)?;

        let attr_args = NestedMeta::parse_meta_list(args)?;

        let mut args = ToolArgs::from_list(&attr_args)?;
        for arg in input.sig.inputs.iter().skip(1) {
            if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
                if let Pat::Ident(ident) = &**pat {
                    let ty = as_owned_ty(ty);

                    // It will error later if the types don't match
                    //
                    // It overwrites any json_type or rust_type set earlier in the attribute macro
                    args.params
                        .iter_mut()
                        .find(|p| ident.ident == p.name)
                        .map(|p| {
                            p.json_type = ParamType::try_from_rust_type(&ty)?;
                            p.rust_type = ty;

                            Ok::<(), Error>(())
                        })
                        .transpose()?;
                }
            }
        }

        validate_spec_and_fn_args_match(&args, input)?;

        args.with_name_from_ident(&input.sig.ident);

        Ok(args)
    }

    pub fn infer_param_types(&mut self) -> Result<(), Error> {
        for param in &mut self.params {
            if param.json_type == ParamType::String
                && param.rust_type != syn::parse_quote! { String }
            {
                param.json_type = ParamType::try_from_rust_type(&param.rust_type)?;
                continue;
            }

            if param.json_type != ParamType::String
                && param.rust_type == syn::parse_quote! { String }
            {
                param.rust_type = param.json_type.default_rust_type();
                continue;
            }

            if ParamType::try_from_rust_type(&param.rust_type)? != param.json_type {
                return Err(Error::custom(format!(
                    "The type of the parameter {} is not compatible with the json type",
                    param.name
                )));
            }
        }
        Ok(())
    }

    pub fn with_name_from_ident(&mut self, ident: &syn::Ident) {
        if self.name.is_empty() {
            self.name = ident.to_string().to_case(Case::Snake);
        }

        if self.fn_name.is_empty() {
            self.fn_name = ident.to_string().to_case(Case::Snake);
        }
    }

    pub fn tool_name(&self) -> &str {
        &self.name
    }

    pub fn fn_name(&self) -> &str {
        &self.fn_name
    }

    pub fn tool_description(&self) -> &Description {
        &self.description
    }

    pub fn tool_params(&self) -> &[ParamOptions] {
        &self.params
    }

    pub fn arg_names(&self) -> Vec<&str> {
        self.params
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
    }

    pub fn args_struct(&self) -> TokenStream {
        if self.params.is_empty() {
            return quote! {};
        }

        let mut fields = Vec::new();

        for param in &self.params {
            let ty = &param.rust_type;
            let ident = syn::Ident::new(&param.name, proc_macro2::Span::call_site());
            fields.push(quote! { pub #ident: #ty });
        }

        let args_struct_ident = self.args_struct_ident();
        quote! {
            #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize, Debug)]
            pub struct #args_struct_ident {
                #(#fields),*
            }
        }
    }

    pub fn args_struct_ident(&self) -> Ident {
        syn::Ident::new(
            &format!("{}Args", self.name.to_case(Case::Pascal)),
            proc_macro2::Span::call_site(),
        )
    }

    #[cfg(test)]
    pub(crate) fn new(name: &str, description: Description, params: Vec<ParamOptions>) -> Self {
        Self {
            name: name.into(),
            fn_name: name.into(),
            description,
            params,
        }
    }
}

fn validate_spec_and_fn_args_match(tool_args: &ToolArgs, item_fn: &ItemFn) -> Result<(), Error> {
    let mut found_spec_arg_names = tool_args
        .params
        .iter()
        .map(|param| param.name.clone())
        .collect::<Vec<_>>();
    found_spec_arg_names.sort();

    let mut seen_arg_names = vec![];

    let mut only_strings = true;
    item_fn
        .sig
        .inputs
        .iter()
        .skip(1)
        .filter_map(|arg| {
            if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
                if let Pat::Ident(ident) = &**pat {
                    seen_arg_names.push(ident.ident.to_string());
                    if let syn::Type::Path(p) = &**ty {
                        if !p.path.is_ident("str") || !p.path.is_ident("String") {
                            only_strings = false;
                        }
                    }

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
    seen_arg_names.sort();

    let mut errors = Error::accumulator();
    if found_spec_arg_names != seen_arg_names {
        let missing_args = found_spec_arg_names
            .iter()
            .filter(|name| !seen_arg_names.contains(name))
            .collect::<Vec<_>>();

        let missing_params = seen_arg_names
            .iter()
            .filter(|name| !found_spec_arg_names.contains(name))
            .collect::<Vec<_>>();

        if !missing_args.is_empty() {
            errors.push(Error::custom(
                format!("The following parameters are missing from the function signature: {missing_args:?}")
            ));
        }

        if !missing_params.is_empty() {
            errors.push(Error::custom(format!(
                "The following parameters are missing from the spec: {missing_params:?}"
            )));
        }
    }

    if !only_strings
        && tool_args
            .params
            .iter()
            .all(|p| matches!(p.json_type, ParamType::String))
    {
        errors.push(Error::custom(
            "Params that are not strings need to have their `type` as json spec specified",
        ));
    }

    errors.finish()?;
    Ok(())
}

pub(crate) fn args_struct_name(input: &ItemFn) -> Ident {
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
    Ident::new(&struct_name_str, input.sig.ident.span())
}

fn as_owned_ty(ty: &syn::Type) -> syn::Type {
    if let syn::Type::Reference(r) = ty {
        if let syn::Type::Path(p) = &*r.elem {
            if p.path.is_ident("str") {
                return parse_quote!(String);
            }

            // Does this happen?
            if p.path.is_ident("Vec") {
                if let syn::PathArguments::AngleBracketed(args) = &p.path.segments[0].arguments {
                    if let syn::GenericArgument::Type(ty) = args.args.first().unwrap() {
                        return as_owned_ty(ty);
                    }
                }
            }
        }
        if let syn::Type::Slice(slice_type) = &*r.elem {
            // slice_type.elem is T. We'll replace with Vec<T>.
            let elem = &slice_type.elem;
            return parse_quote!(Vec<#elem>);
        }
        parse_quote!(String)
    } else {
        ty.to_owned()
    }
}

/// Builds the parse-able arg struct
// pub(crate) fn build_tool_args(input: &ItemFn) -> Result<TokenStream> {
//     validate_first_argument_is_agent_context(input)?;
//
//     let args = &input.sig.inputs;
//     let mut struct_fields = Vec::new();
//
//     for arg in args.iter().skip(1) {
//         if let syn::FnArg::Typed(PatType { pat, ty, .. }) = arg {
//             if let syn::Pat::Ident(ident) = &**pat {
//                 let ty = as_owned_ty(ty);
//                 struct_fields.push(quote! { pub #ident: #ty });
//             }
//         }
//     }
//
//     if struct_fields.is_empty() {
//         return Ok(quote! {});
//     }
//
//     let struct_name = args_struct_name(input);
//
//     Ok(quote! {
//         #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize)]
//         struct #struct_name {
//             #(#struct_fields),*
//         }
//     })
// }

fn validate_first_argument_is_agent_context(input_fn: &ItemFn) -> Result<(), Error> {
    let expected_first_arg = quote! { &dyn AgentContext };
    let error_msg = "The first argument must be `&dyn AgentContext`";

    if let Some(FnArg::Typed(first_arg)) = input_fn.sig.inputs.first() {
        if first_arg.ty.to_token_stream().to_string() != expected_first_arg.to_string() {
            return Err(Error::custom(error_msg).with_span(&first_arg.ty));
        }
    } else {
        return Err(Error::custom(error_msg).with_span(&input_fn.sig));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::assert_ts_eq;

    use super::*;
    use quote::quote;
    use syn::{parse_quote, ItemFn};

    // #[test]
    // fn test_agent_context_as_first_arg_required() {
    //     let input: ItemFn = parse_quote! {
    //         pub async fn search_code(code_query: &str) -> Result<ToolOutput> {
    //             return Ok("hello".into())
    //         }
    //     };
    //
    //     let output = build_tool_args(&input).unwrap_err();
    //
    //     assert_eq!(
    //         output.to_string(),
    //         "The first argument must be `&dyn AgentContext`"
    //     );
    // }
    //
    // #[test]
    // fn test_agent_multiple_args() {
    //     let input: ItemFn = parse_quote! {
    //         pub async fn search_code(context: &dyn AgentContext, code_query: &str, other: &str) -> Result<ToolOutput> {
    //             return Ok("hello".into())
    //         }
    //     };
    //
    //     let output = build_tool_args(&input).unwrap();
    //
    //     let expected = quote! {
    //         #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize)]
    //         struct SearchCodeArgs {
    //             pub code_query: String,
    //             pub other: String
    //         }
    //     };
    //
    //     assert_ts_eq!(&output, &expected);
    // }
    //
    // #[test]
    // fn test_simple_tool_with_lifetime() {
    //     let input: ItemFn = parse_quote! {
    //         pub async fn search_code(context: &dyn AgentContext, code_query: &str) -> Result<ToolOutput> {
    //             return Ok("hello".into())
    //         }
    //     };
    //
    //     let output = build_tool_args(&input).unwrap();
    //
    //     let expected = quote! {
    //         #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize)]
    //         struct SearchCodeArgs {
    //             pub code_query: String,
    //         }
    //     };
    //
    //     assert_ts_eq!(&output, &expected);
    // }
    //
    // #[test]
    // fn test_simple_tool_without_lifetime() {
    //     let input: ItemFn = parse_quote! {
    //         pub async fn search_code(context: &dyn AgentContext, code_query: String) -> Result<ToolOutput> {
    //             return Ok("hello".into())
    //         }
    //     };
    //
    //     let output = build_tool_args(&input).unwrap();
    //
    //     let expected = quote! {
    //         #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize)]
    //         struct SearchCodeArgs {
    //             pub code_query: String,
    //         }
    //     };
    //
    //     assert_ts_eq!(&output, &expected);
    // }
    //
    // #[test]
    // fn test_no_arguments() {
    //     let input: ItemFn = parse_quote! {
    //         pub async fn search_code(context: &dyn AgentContext) -> Result<ToolOutput> {
    //             return Ok("hello".into())
    //         }
    //     };
    //     let output = build_tool_args(&input).unwrap();
    //     let expected = quote! {};
    //     assert_ts_eq!(&output, &expected);
    // }
    //
    // #[test]
    // fn test_multiple_ty_args() {
    //     let input: ItemFn = parse_quote! {
    //         pub async fn search_code(context: &dyn AgentContext, code_query: &str, include_private: bool, a_number: usize, a_slice: &[String], a_vec: Vec<String>) -> Result<ToolOutput> {
    //             return Ok("hello".into())
    //         }
    //     };
    //
    //     let output = build_tool_args(&input).unwrap();
    //     let expected = quote! {
    //         #[derive(::swiftide::reexports::serde::Serialize, ::swiftide::reexports::serde::Deserialize)]
    //         struct SearchCodeArgs {
    //             pub code_query: String,
    //             pub include_private: bool,
    //             pub a_number: usize,
    //             pub a_slice: Vec<String>,
    //             pub a_vec: Vec<String>,
    //         }
    //     };
    //
    //     assert_ts_eq!(&output, &expected);
    // }

    // TODO: Handle no arguments
    // TODO: Should it only allow &str as arg types?
}
