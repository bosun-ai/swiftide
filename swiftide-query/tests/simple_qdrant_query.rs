use swiftide_query::{query, search_strategy::SimilaritySingleEmbedding};

#[test_log::test(tokio::test)]
async fn test_simple_query() {
    let pipeline = query::Pipeline::default()
        .with_search_strategy(SimilaritySingleEmbedding::default())
        .then_transform_query(query_transformers::GenerateSubquestions::default())
        .then_transform_query(query_transformers::Embed::default())
        .then_retrieve(retrievers::Qdrant::default())
        .then_transform_response(response_transformers::Summary::default())
        .then_answer(answers::Simple::default());

    let result = pipeline.query("What is swiftide?");
}
