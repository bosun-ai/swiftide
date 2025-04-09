use darling::{ast::NestedMeta, Error, FromMeta};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, Ident, ItemStruct};

#[derive(FromMeta, Default)]
#[darling(default)]
struct TransformerArgs {
    metadata_field_name: Option<String>,
    default_prompt_file: Option<String>,

    derive: DeriveOptions,
}

#[derive(FromMeta, Debug, Default)]
#[darling(default)]
struct DeriveOptions {
    skip_debug: bool,
    skip_clone: bool,
    skip_default: bool,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn indexing_transformer_impl(args: TokenStream, input: ItemStruct) -> TokenStream {
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(e) => return e.write_errors(),
    };

    let struct_name = &input.ident;
    let builder_name = Ident::new(
        &format!("{struct_name}Builder"),
        proc_macro2::Span::call_site(),
    );
    let vis = &input.vis;
    let attrs = &input.attrs;
    let existing_fields =
        extract_existing_fields(input.fields).collect::<Vec<proc_macro2::TokenStream>>();

    let metadata_field_name = match args.metadata_field_name {
        Some(name) => quote! { pub const NAME: &str = #name; },
        None => quote! {},
    };

    let prompt_template_struct_attr = match &args.default_prompt_file {
        Some(_file) => quote! {
            #[builder(default = "default_prompt()")]
            prompt_template: hidden::Prompt,
        },
        None => quote! {},
    };

    let default_prompt_fn = match &args.default_prompt_file {
        Some(file) => quote! {
            fn default_prompt() -> hidden::Prompt {
                include_str!(#file).into()
            }
        },
        None => quote! {},
    };

    let derive = {
        let mut tokens = vec![quote! { hidden::Builder}];
        if !args.derive.skip_debug {
            tokens.push(quote! { Debug });
        }
        if !args.derive.skip_clone {
            tokens.push(quote! { Clone });
        }

        quote! { #[derive(#(#tokens),*)] }
    };

    let default_impl = if args.derive.skip_default {
        quote! {}
    } else {
        quote! {
            impl Default for #struct_name {
                fn default() -> Self {
                    #builder_name::default().build().unwrap()
                }
            }
        }
    };

    quote! {
        mod hidden {
            pub use std::sync::Arc;
            pub use anyhow::Result;
            pub use derive_builder::Builder;
            pub use swiftide_core::{
                indexing::{IndexingDefaults},
                prompt::Prompt,
                chat_completion::errors::LanguageModelError,
                SimplePrompt, Transformer, WithIndexingDefaults
            };

        }

        #metadata_field_name

        #derive
        #[builder(setter(into, strip_option), build_fn(error = "anyhow::Error"))]
        #(#attrs)*
        #vis struct #struct_name {
            #(#existing_fields)*
            #[builder(setter(custom), default)]
            client: Option<hidden::Arc<dyn hidden::SimplePrompt>>,

            #prompt_template_struct_attr

            #[builder(default)]
            concurrency: Option<usize>,
            #[builder(private, default)]
            indexing_defaults: Option<hidden::IndexingDefaults>,
        }

        #default_impl

        impl #struct_name {
            /// Creates a new builder for the transformer
            pub fn builder() -> #builder_name {
                #builder_name::default()
            }

            /// Build a new transformer from a client
            pub fn from_client(client: impl hidden::SimplePrompt + 'static) -> #builder_name {
                #builder_name::default().client(client).to_owned()
            }

            /// Create a new transformer from a client
            pub fn new(client: impl hidden::SimplePrompt + 'static) -> Self {
                #builder_name::default().client(client).build().unwrap()
            }

            /// Set the concurrency level for the transformer
            #[must_use]
            pub fn with_concurrency(mut self, concurrency: usize) -> Self {
                self.concurrency = Some(concurrency);
                self
            }


            /// Prompts either the client provided to the transformer or a default client
            /// provided on the indexing pipeline
            ///
            /// # Errors
            ///
            /// Gives an error if no (default) client is provided
            async fn prompt(&self, prompt: hidden::Prompt) -> hidden::Result<String, hidden::LanguageModelError> {
                if let Some(client) = &self.client {
                    return client.prompt(prompt).await
                };

                let Some(defaults) = &self.indexing_defaults.as_ref() else {
                    return Err(hidden::LanguageModelError::PermanentError("No client provided".into()))
                };

                let Some(client) = defaults.simple_prompt() else {
                    return Err(hidden::LanguageModelError::PermanentError("No client provided".into()))
                };
                client.prompt(prompt).await
            }
        }

        impl #builder_name {
            pub fn client(&mut self, client: impl hidden::SimplePrompt + 'static) -> &mut Self {
                self.client = Some(Some(hidden::Arc::new(client) as hidden::Arc<dyn hidden::SimplePrompt>));
                self
            }
        }

        impl hidden::WithIndexingDefaults for #struct_name {
            fn with_indexing_defaults(&mut self, defaults: hidden::IndexingDefaults) {
                self.indexing_defaults = Some(defaults);
            }
        }

        #default_prompt_fn
    }
}

fn parse_args(args: TokenStream) -> Result<TransformerArgs, Error> {
    let attr_args = NestedMeta::parse_meta_list(args)?;

    TransformerArgs::from_list(&attr_args)
}

fn extract_existing_fields(fields: Fields) -> impl Iterator<Item = proc_macro2::TokenStream> {
    fields.into_iter().map(|field| {
        let field_name = &field.ident;
        let field_type = &field.ty;
        let field_vis = &field.vis;
        let field_attrs = &field.attrs;

        quote! {
            #(#field_attrs)*
            #field_vis #field_name: #field_type,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use syn::{parse_quote, ItemStruct};

    #[test]
    fn test_includes_doc_comments() {
        let input: ItemStruct = parse_quote! {
            /// This is a test struct
            pub struct TestStruct {
                /// This is a test field
                pub test_field: String,
            }
        };

        let args: TokenStream = quote!();
        let output = indexing_transformer_impl(args, input);

        let expected_output = quote! {
            mod hidden {
                pub use std::sync::Arc;
                pub use anyhow::Result;
                pub use derive_builder::Builder;
                pub use swiftide_core::{
                    indexing::{IndexingDefaults},
                    prompt::Prompt,
                    chat_completion::errors::LanguageModelError,
                    SimplePrompt, Transformer, WithIndexingDefaults
                };
            }

            #[derive(hidden::Builder, Debug, Clone)]
            #[builder(setter(into, strip_option), build_fn(error = "anyhow::Error"))]
            /// This is a test struct
            pub struct TestStruct {
                /// This is a test field
                pub test_field: String,
                #[builder(setter(custom), default)]
                client: Option<hidden::Arc<dyn hidden::SimplePrompt>>,
                #[builder(default)]
                concurrency: Option<usize>,
                #[builder(private, default)]
                indexing_defaults: Option<hidden::IndexingDefaults>,
            }

            impl Default for TestStruct {
                fn default() -> Self {
                    TestStructBuilder::default().build().unwrap()
                }
            }

            impl TestStruct {
                /// Creates a new builder for the transformer
                pub fn builder() -> TestStructBuilder {
                    TestStructBuilder::default()
                }

                /// Build a new transformer from a client
                pub fn from_client(client: impl hidden::SimplePrompt + 'static) -> TestStructBuilder {
                    TestStructBuilder::default().client(client).to_owned()
                }

                /// Create a new transformer from a client
                pub fn new(client: impl hidden::SimplePrompt + 'static) -> Self {
                    TestStructBuilder::default().client(client).build().unwrap()
                }

                /// Set the concurrency level for the transformer
                #[must_use]
                pub fn with_concurrency(mut self, concurrency: usize) -> Self {
                    self.concurrency = Some(concurrency);
                    self
                }

                /// Prompts either the client provided to the transformer or a default client
                /// provided on the indexing pipeline
                ///
                /// # Errors
                ///
                /// Gives an error if no (default) client is provided
                async fn prompt(&self, prompt: hidden::Prompt) -> hidden::Result<String, hidden::LanguageModelError> {
                    if let Some(client) = &self.client {
                        return client.prompt(prompt).await
                    };

                    let Some(defaults) = &self.indexing_defaults.as_ref() else {
                        return Err(hidden::LanguageModelError::PermanentError("No client provided".into()))
                    };

                    let Some(client) = defaults.simple_prompt() else {
                        return Err(hidden::LanguageModelError::PermanentError("No client provided".into()))
                    };
                    client.prompt(prompt).await
                }
            }

            impl TestStructBuilder {
                pub fn client(&mut self, client: impl hidden::SimplePrompt + 'static) -> &mut Self {
                    self.client = Some(Some(hidden::Arc::new(client) as hidden::Arc<dyn hidden::SimplePrompt>));
                    self
                }
            }

            impl hidden::WithIndexingDefaults for TestStruct {
                fn with_indexing_defaults(&mut self, defaults: hidden::IndexingDefaults) {
                    self.indexing_defaults = Some(defaults);
                }
            }
        };

        assert_eq!(output.to_string(), expected_output.to_string());
    }
}
