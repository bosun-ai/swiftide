use darling::{ast::NestedMeta, Error, FromMeta};
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, punctuated::Punctuated, token::Comma, Data, DeriveInput, Field, Fields,
    MetaList,
};

#[derive(FromMeta)]
struct TransformerArgs {
    metadata_field_name: String,
    default_prompt_file: String,
}

pub(crate) fn indexing_transformer_impl(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let args = parse_args(args).unwrap();

    let struct_name = input.ident.to_string();
    let builder_name = format!("{struct_name}Builder");
    let vis = &input.vis;
    let existing_fields = extract_existing_fields(input.data);

    let metadata_field_name = args.metadata_field_name;
    let default_prompt_file = args.default_prompt_file;

    quote! {
        pub const NAME: &str = #metadata_field_name;

        #[derive(Debug, Clone, Builder)]
        #[builder(setter(into, strip_option))]
        #vis #struct_name {
            #(#existing_fields).*
            #[builder(setter(custom))]
            client: Option<Arc<dyn SimplePrompt>>,

            #[builder(default = "default_prompt()")]
            prompt_template: PromptTemplate,
            #[builder(default)]
            concurrency: Option<usize>,
            #[builder(private, default)]
            indexing_defaults: Option<IndexingDefaults>,
        }

        impl #struct_name {
            pub fn builder() -> #builder_name {
                #builder_name::default()
            }
            pub fn build_from_client(client: impl SimplePrompt + 'static) -> #builder_name {
                #builder_name::default().client(client).to_owned()
            }
            pub fn new(client: impl SimplePrompt + 'static) -> Self {
                Self {
                    client: Some(Arc::new(client)),
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

        impl #builder_name {
            pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
                self.client = Some(Arc::new(client));
                self
            }
        }

        fn default_prompt() -> PromptTemplate {
            include_str!(#default_prompt_file).into()
        }
    }
    .into()
}

fn parse_args(args: TokenStream) -> Result<TransformerArgs, Error> {
    let attr_args = NestedMeta::parse_meta_list(args.into())?;

    TransformerArgs::from_list(&attr_args)
}

fn extract_existing_fields(data: Data) -> impl Iterator<Item = proc_macro2::TokenStream> {
    let fields = if let Data::Struct(data_struct) = data {
        if let Fields::Named(fields_named) = data_struct.fields {
            fields_named.named
        } else {
            // Only handle named fields for this macro
            panic!("Expected named fields");
        }
    } else {
        panic!("Expected a struct");
    };

    fields.into_iter().map(|field| {
        let field_name = &field.ident;
        let field_type = &field.ty;
        let field_vis = &field.vis;

        quote! {
            #field_vis #field_name: #field_type
        }
    })
}
