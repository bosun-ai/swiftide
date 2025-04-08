//! Generate an answer based on the current query
use std::sync::Arc;
use swiftide_core::{
    document::Document,
    indexing::SimplePrompt,
    prelude::*,
    prompt::Prompt,
    querying::{states, Query},
    Answer,
};

/// Generate an answer based on the current query
///
/// For most general purposes, this transformer should provide a sensible default. It takes either
/// a transformation that has already been applied to the documents (in `Query::current`), or the
/// documents themselves, and will then feed them as context with the _original_ question to an llm
/// to generate an answer.
///
/// Optionally, a custom document template can be provided to render the documents in a specific
/// way.
#[derive(Debug, Clone, Builder)]
pub struct Simple {
    #[builder(setter(custom))]
    client: Arc<dyn SimplePrompt>,
    #[builder(default = "default_prompt()")]
    prompt_template: Prompt,
    #[builder(default, setter(into, strip_option))]
    document_template: Option<Prompt>,
}

impl Simple {
    pub fn builder() -> SimpleBuilder {
        SimpleBuilder::default()
    }

    /// Builds a new simple answer generator from a client that implements [`SimplePrompt`].
    ///
    /// # Panics
    ///
    /// Panics if the build failed
    pub fn from_client(client: impl SimplePrompt + 'static) -> Simple {
        SimpleBuilder::default()
            .client(client)
            .to_owned()
            .build()
            .expect("Failed to build Simple")
    }
}

impl SimpleBuilder {
    pub fn client(&mut self, client: impl SimplePrompt + 'static) -> &mut Self {
        self.client = Some(Arc::new(client) as Arc<dyn SimplePrompt>);
        self
    }
}

fn default_prompt() -> Prompt {
    indoc::indoc! {"
    Answer the following question based on the context provided:
    {{ question }}

    ## Constraints
    * Do not include any information that is not in the provided context.
    * If the question cannot be answered by the provided context, state that it cannot be answered.
    * Answer the question completely and format it as markdown.

    ## Context

    ---
    {{ documents }}
    ---
    "}
    .into()
}

#[async_trait]
impl Answer for Simple {
    #[tracing::instrument(skip_all)]
    async fn answer(&self, query: Query<states::Retrieved>) -> Result<Query<states::Answered>> {
        let mut context = tera::Context::new();

        context.insert("question", query.original());

        let documents = if !query.current().is_empty() {
            query.current().to_string()
        } else if let Some(template) = &self.document_template {
            let mut rendered_documents = Vec::new();
            for document in query.documents() {
                let rendered = template
                    .clone()
                    .with_context(tera::Context::from_serialize(document)?)
                    .render()?;
                rendered_documents.push(rendered);
            }

            rendered_documents.join("\n---\n")
        } else {
            query
                .documents()
                .iter()
                .map(Document::content)
                .collect::<Vec<_>>()
                .join("\n---\n")
        };
        context.insert("documents", &documents);

        let answer = self
            .client
            .prompt(self.prompt_template.clone().with_context(context))
            .await?;

        Ok(query.answered(answer))
    }
}

#[cfg(test)]
mod test {
    use std::sync::Mutex;

    use insta::assert_snapshot;
    use swiftide_core::{indexing::Metadata, MockSimplePrompt};

    use super::*;

    assert_default_prompt_snapshot!("question" => "What is love?", "documents" => "My context");

    #[tokio::test]
    async fn test_uses_current_if_present() {
        let mut mock_client = MockSimplePrompt::new();

        // I'll buy a beer for the first person who can think of a less insane way to do this
        let received_prompt = Arc::new(Mutex::new(None));
        let cloned = received_prompt.clone();
        mock_client
            .expect_prompt()
            .withf(move |prompt| {
                cloned.lock().unwrap().replace(prompt.clone());
                true
            })
            .once()
            .returning(|_| Ok(String::default()));

        let documents = vec![
            Document::new("First document", Some(Metadata::from(("some", "metadata")))),
            Document::new(
                "Second document",
                Some(Metadata::from(("other", "metadata"))),
            ),
        ];
        let query: Query<states::Retrieved> = Query::builder()
            .original("original")
            .current("A fictional generated summary")
            .state(states::Retrieved)
            .documents(documents)
            .build()
            .unwrap();

        let transformer = Simple::builder().client(mock_client).build().unwrap();

        transformer.answer(query).await.unwrap();

        let received_prompt = received_prompt.lock().unwrap().take().unwrap();
        let rendered = received_prompt.render().unwrap();
        assert_snapshot!(rendered);
    }

    #[tokio::test]
    async fn test_custom_document_template() {
        let mut mock_client = MockSimplePrompt::new();

        // I'll buy a beer for the first person who can think of a less insane way to do this
        let received_prompt = Arc::new(Mutex::new(None));
        let cloned = received_prompt.clone();
        mock_client
            .expect_prompt()
            .withf(move |prompt| {
                cloned.lock().unwrap().replace(prompt.clone());
                true
            })
            .once()
            .returning(|_| Ok(String::default()));

        let documents = vec![
            Document::new("First document", Some(Metadata::from(("some", "metadata")))),
            Document::new(
                "Second document",
                Some(Metadata::from(("other", "metadata"))),
            ),
        ];
        let query: Query<states::Retrieved> = Query::builder()
            .original("original")
            .current(String::default())
            .state(states::Retrieved)
            .documents(documents)
            .build()
            .unwrap();

        let transformer = Simple::builder()
            .client(mock_client)
            .document_template(indoc::indoc! {"
                {% for key, value in metadata -%}
                    {{ key }}: {{ value }}
                {% endfor -%}

                {{ content }}"})
            .build()
            .unwrap();

        transformer.answer(query).await.unwrap();

        let received_prompt = received_prompt.lock().unwrap().take().unwrap();
        let rendered = received_prompt.render().unwrap();
        assert_snapshot!(rendered);
    }
}
