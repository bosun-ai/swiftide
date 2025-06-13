use std::borrow::Cow;

use metrics::{IntoLabels, Label, counter, histogram};

use crate::metadata::Metadata;

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

    counter!("swiftide.usage.prompt_tokens", metadata.iter()).increment(prompt_tokens);
    counter!("swiftide.usage.completion_tokens", metadata.iter()).increment(completion_tokens);
    counter!("swiftide.usage.total_tokens", metadata.iter()).increment(total_tokens);
}
