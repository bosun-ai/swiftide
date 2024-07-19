#[test_log::test(tokio::test)]
async fn test_simple_query() {
    let pipeline = query::Pipeline::new()
        .with_search_strategy(SearchStrategy::default())
        .then_transform_query(query_transformers::GenerateSubquestions::default())
        .then_transform_query(query_transformers::Embed::default())
        .then_retrieve(retrievers::Qdrant::default())
        .transform_response(response_transformers::Summary::default())

    let result = pipeline
        .query("What is swiftide?")
}
