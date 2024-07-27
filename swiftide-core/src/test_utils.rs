#[macro_export]
macro_rules! assert_default_prompt_snapshot {
    ($node:expr, $($key:expr => $value:expr),*) => {
        #[tokio::test]
        async fn test_default_prompt() {
        let template = default_prompt();
        let mut prompt = template.to_prompt().with_node(&Node::new($node));
        $(
            prompt = prompt.with_context_value($key, $value);
        )*
        insta::assert_snapshot!(prompt.render().await.unwrap());
        }
    };

    ($($key:expr => $value:expr),*) => {
        #[tokio::test]
        async fn test_default_prompt() {
            let template = default_prompt();
            let mut prompt = template.to_prompt();
            $(
                prompt = prompt.with_context_value($key, $value);
            )*
            insta::assert_snapshot!(prompt.render().await.unwrap());
        }
    };
}
