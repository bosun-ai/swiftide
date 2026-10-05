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

        let mut attr_args = NestedMeta::parse_meta_list(args)?;
        let doc_comments = ToolDocComments::from_fn(input);
        if !has_explicit_description(&attr_args)
            && let Some(description) = &doc_comments.description
        {
            attr_args.extend(NestedMeta::parse_meta_list(
                quote!(description = #description),
            )?);
        }

        let mut args = ToolArgs::from_list(&attr_args)?;
        args.apply_doc_comments(doc_comments);
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

    fn apply_doc_comments(&mut self, parsed: ToolDocComments) {
        if matches!(&self.description, Description::Literal(value) if value.is_empty())
            && let Some(description) = parsed.description.as_ref()
        {
            self.description = Description::Literal(description.clone());
        }

        for (name, description) in parsed.parameters {
            if let Some(param) = self.params.iter_mut().find(|param| param.name == name) {
                if param.description.is_empty() {
                    param.description = description;
                }
            } else {
                self.params.push(ParamOptions {
                    name,
                    description,
                    ..ParamOptions::default()
                });
            }
        }
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

    pub fn derive_invoke_args(&self) -> Vec<TokenStream> {
        self.params
            .iter()
            .map(|param| {
                let ident = syn::Ident::new(&param.name, proc_macro2::Span::call_site());
                if param.should_pass_owned() {
                    quote! { args.#ident }
                } else {
                    quote! { &args.#ident }
                }
            })
            .collect()
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
            let description = &param.description;
            let ident = syn::Ident::new(&param.name, proc_macro2::Span::call_site());
            fields.push(quote! {
                #[schemars(description = #description)]
                pub #ident: #ty
            });
        }

        let args_struct_ident = self.args_struct_ident();
        quote! {
            #[derive(
                ::swiftide::reexports::serde::Serialize,
                ::swiftide::reexports::serde::Deserialize,
                ::swiftide::reexports::schemars::JsonSchema,
                Debug
            )]
            #[schemars(crate = "::swiftide::reexports::schemars", deny_unknown_fields)]
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

#[derive(Default)]
struct ToolDocComments {
    description: Option<String>,
    parameters: Vec<(String, String)>,
}

impl ToolDocComments {
    fn from_fn(input: &ItemFn) -> Self {
        let docs = input
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("doc"))
            .filter_map(|attr| match &attr.meta {
                syn::Meta::NameValue(meta) => match &meta.value {
                    syn::Expr::Lit(expr) => match &expr.lit {
                        syn::Lit::Str(value) => Some(value.value()),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        Self::parse(&docs)
    }

    fn parse(lines: &[String]) -> Self {
        let mut docs = Self::default();
        let mut description_lines = Vec::new();
        let mut in_arguments = false;
        let mut before_sections = true;

        for line in lines {
            let line = line.trim();
            if let Some(heading) = line.strip_prefix('#') {
                let heading = heading.trim_start_matches('#').trim();
                in_arguments = heading.eq_ignore_ascii_case("arguments")
                    || heading.eq_ignore_ascii_case("parameters");
                before_sections = false;
                continue;
            }

            if in_arguments {
                if let Some(parameter) = parse_parameter_doc_line(line) {
                    docs.parameters.push(parameter);
                }
            } else if before_sections && !line.is_empty() {
                description_lines.push(line);
            }
        }

        if !description_lines.is_empty() {
            docs.description = Some(description_lines.join(" "));
        }

        docs
    }
}

fn has_explicit_description(args: &[NestedMeta]) -> bool {
    args.iter().any(|arg| {
        matches!(arg, NestedMeta::Meta(syn::Meta::NameValue(value)) if value.path.is_ident("description"))
    })
}

fn parse_parameter_doc_line(line: &str) -> Option<(String, String)> {
    let line = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))?;
    let (name, description) = if let Some(rest) = line.strip_prefix('`') {
        let (name, rest) = rest.split_once('`')?;
        let description = rest
            .trim_start()
            .strip_prefix(':')
            .or_else(|| rest.trim_start().strip_prefix('-'))?;
        (name, description)
    } else {
        line.split_once(':')?
    };
    let name = name.trim();
    let description = description.trim();
    if name.is_empty() || description.is_empty() {
        return None;
    }

    Some((name.to_owned(), description.to_owned()))
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

fn is_vec_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if type_path.qself.is_some() {
            return false;
        }

        return type_path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Vec");
    }

    false
}

impl ParamOptions {
    fn should_pass_owned(&self) -> bool {
        self.resolved_type.as_ref().is_some_and(is_vec_type)
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

#[cfg(test)]
mod doc_comment_tests {
    use super::ToolDocComments;

    #[test]
    fn parses_summary_and_argument_bullets() {
        let docs = [
            " Search indexed documents.".to_owned(),
            String::new(),
            " # Arguments".to_owned(),
            " - `query`: Text to search for.".to_owned(),
            " - `limit` - Maximum number of results.".to_owned(),
            " # Returns".to_owned(),
            " A list of matching documents.".to_owned(),
        ];

        let parsed = ToolDocComments::parse(&docs);

        assert_eq!(
            parsed.description.as_deref(),
            Some("Search indexed documents.")
        );
        assert_eq!(
            parsed.parameters,
            vec![
                ("query".to_owned(), "Text to search for.".to_owned()),
                ("limit".to_owned(), "Maximum number of results.".to_owned()),
            ]
        );
    }
}
