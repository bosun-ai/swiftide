use std::sync::OnceLock;

use metrics::{IntoLabels, Label, counter, describe_counter};

static METRICS_INIT: OnceLock<bool> = OnceLock::new();

/// Lazily describes all the metrics used in this module once
pub fn lazy_init() {
    METRICS_INIT.get_or_init(|| {
        describe_counter!("swiftide.usage.prompt_tokens", "token usage for the prompt");
        describe_counter!(
            "swiftide.usage.completion_tokens",
            "token usage for the completion"
        );
        describe_counter!("swiftide.usage.total_tokens", "total token usage");
        true
    });
}

/// Emits usage metrics for a language model
pub fn emit_usage(
    model: &str,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    custom_metadata: Option<impl IntoLabels>,
) {
    let model = model.to_string();
    let mut metadata = vec![];

    if let Some(custom_metadata) = custom_metadata {
        metadata.extend(custom_metadata.into_labels());
    }
    metadata.push(Label::new("model", model));

    lazy_init();
    counter!("swiftide.usage.prompt_tokens", metadata.iter()).increment(prompt_tokens);
    counter!("swiftide.usage.completion_tokens", metadata.iter()).increment(completion_tokens);
    counter!("swiftide.usage.total_tokens", metadata.iter()).increment(total_tokens);
}
