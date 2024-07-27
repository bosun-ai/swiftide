use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, LitBool};

#[proc_macro_derive(IndexingTransformer, attributes(transformer_options))]
pub fn indexing_transformer_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let builder_name = format_ident!("{}Builder", name);

    let mut derive_clone = true;
    let mut derive_debug = true;
    let mut derive_builder = true;

    // Optionally disable default derives
    for attr in &input.attrs {
        if attr.path().is_ident("transformer_options") {
            attr.parse_nested_meta(|meta| {
                let lit: LitBool = meta.input.parse().unwrap();
                if meta.path.is_ident("derive_clone") {
                    derive_clone = lit.value();
                    Ok(())
                } else if meta.path.is_ident("derive_debug") {
                    derive_debug = lit.value();
                    Ok(())
                } else if meta.path.is_ident("derive_builder") {
                    derive_builder = lit.value();
                    Ok(())
                } else {
                    Ok(())
                }
            });
        }
    }

    // Derive everything that is requested
    let derives = {
        let mut derive_tokens = vec![];
        if derive_debug {
            derive_tokens.push(quote! { Debug });
        }
        if derive_clone {
            derive_tokens.push(quote! { Clone });
        }
        if derive_builder {
            derive_tokens.push(quote! { Builder });
        }
        quote! { #[derive(#(#derive_tokens),*)] }
    };

    // Common fields for all transformers
    // TODO: Default prompt should also be optional maybe
    // or skipable
    let common_fields = quote! {
        #[builder(setter(custom))]
        client: std::sync::Arc<dyn SimplePrompt>,
        #[builder(default = "default_prompt()")]
        prompt_template: PromptTemplate,
        #[builder(default)]
        concurrency: Option<usize>,
    };

    let mut builder_methods = quote! {};
    let mut struct_fields = quote! {};

    if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(fields_named) = &data_struct.fields {
            for field in fields_named.named.iter() {
                let field_name = &field.ident;
                let field_type = &field.ty;

                struct_fields = quote! {
                    #struct_fields
                    #field_name: #field_type,
                };

                if field_name.as_ref().unwrap() == "client" {
                    builder_methods = quote! {
                        #builder_methods
                        pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
                            self.client = Some(std::sync::Arc::new(client));
                            self
                        }
                    };
                }
            }
        }
    }

    let expanded = quote! {
        use std::sync::Arc;
        use async_trait::async_trait;
        use derive_builder::Builder;
        use swiftide_core::{prompt::PromptTemplate, SimplePrompt};

        #derives
        #[builder(setter(into, strip_option))]
        pub struct #name {
            #common_fields
            #struct_fields
        }

        impl #name {
            pub fn builder() -> #builder_name {
                #builder_name::default()
            }

            pub fn from_client(client: impl SimplePrompt + 'static) -> #builder_name {
                #builder_name::default().client(client).to_owned()
            }

            pub fn new(client: impl SimplePrompt + 'static) -> Self {
                Self {
                    client: std::sync::Arc::new(client),
                    prompt_template: default_prompt(),
                    concurrency: None,
                }
            }

            #[must_use]
            pub fn with_concurrency(mut self, concurrency: usize) -> Self {
                self.concurrency = Some(concurrency);
                self
            }
        }

        fn default_prompt() -> PromptTemplate {
            include_str!("prompts/default.prompt.md").into()
        }

        impl #builder_name {
            #builder_methods
        }
    };

    TokenStream::from(expanded)
}
