use convert_case::{Case, Casing as _};
use darling::{Error, FromMeta, ast::NestedMeta};
use proc_macro2::TokenStream;
use quote::{ToTokens as _, quote};
use syn::{FnArg, Ident, ItemFn, Pat, PatType, parse_quote};

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

#[derive(FromMeta, Debug, Default)]
#[darling(default)]
pub struct ParamOptions {
    pub name: String,
    pub description: String,

    /// Backwards compatibility: optional JSON type hint (string based)
    pub json_type: Option<String>,

    /// Explicit rust type override parsed from the attribute
    pub rust_type: Option<syn::Type>,

    pub required: Option<bool>,

    #[darling(skip)]
    pub resolved_type: Option<syn::Type>,
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
            if let FnArg::Typed(PatType { pat, ty, .. }) = arg
                && let Pat::Ident(ident) = &**pat
            {
                let ty = as_owned_ty(ty);

                if let Some(param) = args.params.iter_mut().find(|p| ident.ident == p.name) {
                    param.rust_type = Some(ty);
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
            let mut ty = if let Some(ty) = param.rust_type.clone() {
                ty
            } else if let Some(json_type) = &param.json_type {
                json_type_to_rust_type(json_type)
            } else {
                syn::parse_quote! { String }
            };

            let is_option = is_option_type(&ty);

            match param.required {
                Some(true) if is_option => {
                    return Err(Error::custom(format!(
                        "The parameter {} is marked as required but has an optional type",
                        param.name
                    )));
                }
                Some(false) if !is_option => {
                    ty = wrap_type_in_option(ty);
                }
                None if is_option => {
                    param.required = Some(false);
                }
                None => {
                    param.required = Some(true);
                }
                _ => {}
            }

            param.resolved_type = Some(ty);
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
            let ty = param
                .resolved_type
                .as_ref()
                .expect("parameter types should be resolved");
            let ident = syn::Ident::new(&param.name, proc_macro2::Span::call_site());
            fields.push(quote! { pub #ident: #ty });
        }

        let args_struct_ident = self.args_struct_ident();
        quote! {
            #[derive(
                ::swiftide::reexports::serde::Serialize,
                ::swiftide::reexports::serde::Deserialize,
                ::swiftide::reexports::schemars::JsonSchema,
                Debug
            )]
            #[schemars(crate = "::swiftide::reexports::schemars")]
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
        if let FnArg::Typed(PatType { pat, .. }) = arg
            && let Pat::Ident(ident) = &**pat
        {
            seen_arg_names.push(ident.ident.to_string());
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
            errors.push(Error::custom(format!(
                "The following parameters are missing from the function signature: {missing_args:?}"
            )));
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

fn json_type_to_rust_type(json_type: &str) -> syn::Type {
    match json_type.to_ascii_lowercase().as_str() {
        "number" => syn::parse_quote! { usize },
        "boolean" => syn::parse_quote! { bool },
        "array" => syn::parse_quote! { Vec<String> },
        "object" => syn::parse_quote! { ::serde_json::Value },
        // default to string if nothing is specified
        _ => syn::parse_quote! { String },
    }
}

fn is_option_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if type_path.qself.is_some() {
            return false;
        }

        return type_path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Option");
    }

    false
}

fn wrap_type_in_option(ty: syn::Type) -> syn::Type {
    if is_option_type(&ty) {
        ty
    } else {
        syn::parse_quote! { Option<#ty> }
    }
}

fn as_owned_ty(ty: &syn::Type) -> syn::Type {
    if let syn::Type::Reference(r) = ty {
        if let syn::Type::Path(p) = &*r.elem {
            if p.path.is_ident("str") {
                return parse_quote!(String);
            }

            // Does this happen?
            if p.path.is_ident("Vec")
                && let syn::PathArguments::AngleBracketed(args) = &p.path.segments[0].arguments
                && let syn::GenericArgument::Type(ty) = args.args.first().unwrap()
            {
                let inner = as_owned_ty(ty);
                return parse_quote!(Vec<#inner>);
            }

            if let Some(last_segment) = p.path.segments.last()
                && last_segment.ident.to_string().as_str() == "Option"
                && let syn::PathArguments::AngleBracketed(generics) = &last_segment.arguments
                && let Some(syn::GenericArgument::Type(inner_ty)) = generics.args.first()
            {
                let inner_ty = as_owned_ty(inner_ty);
                return parse_quote!(Option<#inner_ty>);
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
