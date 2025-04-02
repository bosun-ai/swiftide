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

    pub required: bool,
}

impl Default for ParamOptions {
    fn default() -> Self {
        ParamOptions {
            name: String::new(),
            description: String::new(),
            json_type: ParamType::String,
            rust_type: syn::parse_quote! { String },
            required: true,
        }
    }
}

#[derive(Debug, FromMeta, PartialEq, Eq, Default, Clone)]
#[darling(rename_all = "camelCase")]
pub enum ParamType {
    #[default]
    String,
    Number,
    Boolean,
    Array,
    #[darling(skip)]
    Option(Box<ParamType>),
}

impl ParamType {
    fn default_rust_type(&self) -> syn::Type {
        match self {
            ParamType::String => syn::parse_quote! { String },
            ParamType::Number => syn::parse_quote! { usize },
            ParamType::Boolean => syn::parse_quote! { bool },
            ParamType::Array => syn::parse_quote! { Vec<String> },
            ParamType::Option(t) => {
                let inner_ty = t.default_rust_type();
                syn::parse_quote! {Option<#inner_ty>}
            }
        }
    }

    fn try_from_rust_type(ty: &syn::Type) -> Result<ParamType, Error> {
        rust_type_to_json_type(ty)
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
                            p.required = !matches!(p.json_type, ParamType::Option(..));

                            Ok::<(), Error>(())
                        })
                        .transpose()?;
                }
            }
        }
        args.infer_param_types()?;

        validate_spec_and_fn_args_match(&args, input)?;

        args.with_name_from_ident(&input.sig.ident);

        Ok(args)
    }

    pub fn infer_param_types(&mut self) -> Result<(), Error> {
        for param in &mut self.params {
            // Just be flexible. Might be weird if required is explicitly set to true. But it's
            // more lenient if the user just provides the rust type.
            if matches!(param.json_type, ParamType::Option(..))
                || param
                    .rust_type
                    .to_token_stream()
                    .to_string()
                    .contains("Option")
            {
                param.required = false;
            }

            if param.required {
                if matches!(param.json_type, ParamType::Option(..)) {
                    return Err(Error::custom(format!(
                        "The parameter {} is marked as required but is an option",
                        param.name
                    )));
                }

                if param
                    .rust_type
                    .to_token_stream()
                    .to_string()
                    .contains("Option")
                {
                    return Err(Error::custom(format!(
                        "The parameter {} is marked as required but is an option",
                        param.name
                    )));
                }

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
            } else {
                // They are the same but no option, so let's wrap them both
                if param.json_type == ParamType::try_from_rust_type(&param.rust_type)?
                    && !matches!(param.json_type, ParamType::Option(..))
                {
                    let option_param = ParamType::Option(Box::new(param.json_type.clone()));
                    let rust_ty = param.rust_type.clone();
                    param.rust_type = parse_quote!(Option<#rust_ty>);
                    param.json_type = option_param;
                    continue;
                }

                if param.json_type == ParamType::String
                    && param.rust_type != syn::parse_quote! { String }
                {
                    param.json_type = ParamType::try_from_rust_type(&param.rust_type)?;
                    continue;
                }

                if param.json_type != ParamType::String
                    && param.rust_type == syn::parse_quote! { String }
                {
                    let option_param = ParamType::Option(Box::new(param.json_type.clone()));
                    let rust_ty = param.rust_type.clone();
                    param.rust_type = parse_quote!(Option<#rust_ty>);
                    param.json_type = option_param;
                    continue;
                }
            }

            if ParamType::try_from_rust_type(&param.rust_type)? != param.json_type {
                return Err(Error::custom(format!(
                    "The type of the parameter {} is not compatible with the json type; if it is an option make sure you set `required` to false in the param attribute",
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
}

fn validate_spec_and_fn_args_match(tool_args: &ToolArgs, item_fn: &ItemFn) -> Result<(), Error> {
    let mut found_spec_arg_names = tool_args
        .params
        .iter()
        .map(|param| param.name.clone())
        .collect::<Vec<_>>();
    found_spec_arg_names.sort();

    let mut seen_arg_names = vec![];

    item_fn.sig.inputs.iter().skip(1).for_each(|arg| {
        if let FnArg::Typed(PatType { pat, .. }) = arg {
            if let Pat::Ident(ident) = &**pat {
                seen_arg_names.push(ident.ident.to_string());
            }
        }
    });
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

    errors.finish()?;
    Ok(())
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
                        let inner = as_owned_ty(ty);
                        return parse_quote!(Vec<#inner>);
                    }
                }
            }

            if let Some(last_segment) = p.path.segments.last() {
                if last_segment.ident.to_string().as_str() == "Option" {
                    if let syn::PathArguments::AngleBracketed(generics) = &last_segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner_ty)) = generics.args.first() {
                            let inner_ty = as_owned_ty(inner_ty);
                            return parse_quote!(Option<#inner_ty>);
                        }
                    }
                }
            }

            return parse_quote!(String);
        }
        if let syn::Type::Slice(slice_type) = &*r.elem {
            // slice_type.elem is T. We'll replace with Vec<T>.
            let elem = &slice_type.elem;
            return parse_quote!(Vec<#elem>);
        }
        panic!("Unsupported reference type");
    } else {
        ty.to_owned()
    }
}

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
