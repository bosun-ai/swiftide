use anyhow::Result;
use indoc::formatdoc;
use infrastructure::Embed;
use infrastructure::SimplePrompt;
use qdrant_client::{client::QdrantClient, qdrant::SearchPoints};

pub async fn query(query: &str) -> Result<String> {
    let client = QdrantClient::from_url("http://localhost:6334")
        .build()
        .unwrap();

    let openai = infrastructure::create_openai_client();

    let mut embedded_query = openai
        .embed(vec![query.to_string()], "text-embedding-3-small")
        .await?;

    let search_result = client
        .search_points(&SearchPoints {
            collection_name: "latest-test".to_string(),
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

    let prompt = formatdoc!(
        r#"
        Answer the following question:
        {query}

        ## Constraints
        * Only answer based on the provided context below
        * Be elaborate and specific in your answers
        
        ## Additional information found
        {result_context}
        "#,
    );

    openai.prompt(&prompt, "gpt-4o").await
}
