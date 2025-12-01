use anyhow::anyhow;

use swiftide_core::chat_completion::ChatMessage;

use crate::openai::LanguageModelError;
use crate::openai::token_estimator::TokenEstimator;

/// Strategy to handle prompts that exceed a target token budget.
#[derive(Debug, Clone)]
pub enum TokenOverflowStrategy {
    /// Remove lines from the end of the last textual message until the prompt
    /// plus the reserved completion budget fits within `max_total_tokens`.
    /// A suffix `"...more {n} lines"` is appended when truncation occurs.
    TruncateLast {
        max_total_tokens: u32,
        max_completion_tokens: u32,
    },
}

impl TokenOverflowStrategy {
    /// Apply the strategy in-place, returning an optional enforced completion budget.
    pub async fn apply(
        &self,
        messages: &mut Vec<ChatMessage>,
        estimator: &(dyn TokenEstimator + Send + Sync),
    ) -> Result<Option<u32>, LanguageModelError> {
        match self {
            TokenOverflowStrategy::TruncateLast {
                max_total_tokens,
                max_completion_tokens,
            } => {
                ensure_truncate_last(
                    messages,
                    estimator,
                    *max_total_tokens,
                    *max_completion_tokens,
                )
                .await?;
                Ok(Some(*max_completion_tokens))
            }
        }
    }
}

async fn ensure_truncate_last(
    messages: &mut Vec<ChatMessage>,
    estimator: &(dyn TokenEstimator + Send + Sync),
    max_total_tokens: u32,
    completion_budget: u32,
) -> Result<(), LanguageModelError> {
    let mut prompt_tokens = estimate_u32(estimator, messages).await?;

    if prompt_tokens + completion_budget <= max_total_tokens {
        return Ok(());
    }

    // Walk messages from the end to find something truncatable.
    let mut idx = None;
    for i in (0..messages.len()).rev() {
        if extract_text_mut(&mut messages[i]).is_some() {
            idx = Some(i);
            break;
        }
    }

    let Some(idx) = idx else {
        return Err(LanguageModelError::context_length_exceeded(anyhow!(
            "No textual message available to truncate"
        )));
    };

    loop {
        let lines: Vec<String> = {
            let msg = messages.get_mut(idx).ok_or_else(|| {
                LanguageModelError::context_length_exceeded("message index missing")
            })?;

            let text_ref = extract_text_mut(msg).ok_or_else(|| {
                LanguageModelError::context_length_exceeded("message cannot be truncated")
            })?;

            text_ref.lines().map(ToString::to_string).collect()
        };

        if lines.is_empty() {
            return Err(LanguageModelError::context_length_exceeded(anyhow!(
                "Last textual message is empty"
            )));
        }

        for remove in 1..=lines.len() {
            let keep = lines.len() - remove;

            let mut new_content = String::new();
            if keep > 0 {
                new_content.push_str(&lines[..keep].join("\n"));
                if !new_content.ends_with('\n') {
                    new_content.push('\n');
                }
            }
            new_content.push_str(&format!("...more {} lines", remove));

            {
                let msg = messages.get_mut(idx).ok_or_else(|| {
                    LanguageModelError::context_length_exceeded("message index missing")
                })?;
                let text_ref = extract_text_mut(msg).ok_or_else(|| {
                    LanguageModelError::context_length_exceeded("message cannot be truncated")
                })?;
                *text_ref = new_content;
            }

            prompt_tokens = estimate_u32(estimator, messages).await?;

            if prompt_tokens + completion_budget <= max_total_tokens {
                return Ok(());
            }
        }

        // If we failed to fit by truncating this message entirely, give up.
        return Err(LanguageModelError::context_length_exceeded(anyhow!(
            "Unable to truncate last message to fit token budget"
        )));
    }
}

fn extract_text_mut(message: &mut ChatMessage) -> Option<&mut String> {
    match message {
        ChatMessage::System(s) | ChatMessage::User(s) | ChatMessage::Summary(s) => Some(s),
        ChatMessage::Assistant(s, _) => s.as_mut(),
        ChatMessage::ToolOutput(_, _) => None,
    }
}

async fn estimate_u32(
    estimator: &(dyn TokenEstimator + Send + Sync),
    messages: &[ChatMessage],
) -> Result<u32, LanguageModelError> {
    let tokens = estimator
        .estimate_chat_messages(messages)
        .await
        .map_err(LanguageModelError::permanent)?;

    u32::try_from(tokens).map_err(|_| LanguageModelError::permanent("token estimate overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::token_estimator::CharEstimator;
    use swiftide_core::chat_completion::ChatMessage;

    fn make_estimator() -> crate::openai::token_estimator::CoreEstimator<CharEstimator> {
        crate::openai::token_estimator::CoreEstimator(CharEstimator::default())
    }

    #[tokio::test]
    async fn truncates_last_message() {
        let mut messages = vec![
            ChatMessage::System("Keep".into()),
            ChatMessage::User("line1\nline2\nline3".into()),
        ];

        let strat = TokenOverflowStrategy::TruncateLast {
            max_total_tokens: 80,
            max_completion_tokens: 10,
        };

        let mut messages = messages;
        let _ = strat.apply(&mut messages, &make_estimator()).await.unwrap();

        let est = make_estimator()
            .estimate_chat_messages(&messages)
            .await
            .unwrap();
        assert!(est + 10 <= 80);
    }

    #[tokio::test]
    async fn errors_when_nothing_to_truncate() {
        let mut messages = vec![ChatMessage::ToolOutput(
            swiftide_core::chat_completion::ToolCall::builder()
                .id("1")
                .name("noop")
                .args("{}")
                .build()
                .unwrap(),
            swiftide_core::chat_completion::ToolOutput::Text("x".into()),
        )];

        let strat = TokenOverflowStrategy::TruncateLast {
            max_total_tokens: 1,
            max_completion_tokens: 1,
        };

        let err = strat
            .apply(&mut messages, &make_estimator())
            .await
            .unwrap_err();

        assert!(matches!(err, LanguageModelError::ContextLengthExceeded(_)));
    }
}
