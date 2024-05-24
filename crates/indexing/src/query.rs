use anyhow::Result;
use indoc::formatdoc;
use infrastructure::Embed;
use infrastructure::SimplePrompt;
use qdrant_client::qdrant::SearchPoints;

/// Performs a naive search using qdrant and openai
///
/// When we add more complicated rag query
/// logic, nice to have a pipeline similar to ingestion and abstract away over the storage.
///
/// This is just quick and dirty so we can get databuoy out.
#[tracing::instrument(
    skip(query, storage_namespace),
    fields(query, response),
    err,
    name = "indexing.query.naieve"
)]
pub async fn naive(query: &str, storage_namespace: &str) -> Result<String> {
    let qdrant = infrastructure::create_qdrant_client()?;
    let openai = infrastructure::create_openai_client();

    let embedding_model = infrastructure::DEFAULT_OPENAI_EMBEDDING_MODEL;

    let mut embedded_query = openai
        .embed(vec![query.to_string()], embedding_model)
        .await?;

    let search_result = qdrant
        .search_points(&SearchPoints {
            collection_name: storage_namespace.to_string(),
            vector: embedded_query
                .drain(0..1)
                .next()
                .ok_or(anyhow::anyhow!("No query vector"))?,
            limit: 10,
            with_payload: Some(true.into()),
            ..Default::default()
        })
        .await?;

    let result_context = search_result
        .result
        .into_iter()
        .fold(String::new(), |acc, point| {
            point
                .payload
                .into_iter()
                .fold(acc, |acc, (k, v)| format!("{}\n{}: {}", acc, k, v))
        });

    tracing::Span::current().record("query", query);

    let prompt = formatdoc!(
        r#"
        Answer the following question(s):
        {query}

        ## Constraints
        * Only answer based on the provided context below
        * Answer the question fully and remember to be concise
        
        ## Additional information found
        {result_context}
        "#,
    );

    let response = openai
        .prompt(&prompt, infrastructure::DEFAULT_OPENAI_MODEL)
        .await?;

    tracing::Span::current().record("response", &response);

    Ok(response)
}
