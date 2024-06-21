use anyhow::{Context as _, Result};
use std::fmt::{Debug, Formatter};

use async_trait::async_trait;
use derive_builder::Builder;
use mistralrs::{Constraint, NormalRequest, Request, RequestMessage, Response, SamplingParams};
use tokio::sync::mpsc::{channel, Sender};

use crate::SimplePrompt;

/// A prompt that uses Mistral.rs to generate completions
///
/// This is a thin wrapper around the Mistral.rs library, which
/// provides implementations for several HuggingFace models. It requires a sender
/// from a Mistral.rs model to send requests.
///
/// See https://github.com/EricLBuehler/mistral.rs for more information.
///
/// Note: By default this feature is disabled as it has a large set of specific ML dependencies.
/// It needs to be explicitly enabled by setting the `huggingface-mistralrs` feature flag.
#[derive(Builder)]
#[builder(pattern = "owned")]
pub struct MistralPrompt {
    sender: Sender<Request>,
}

impl MistralPrompt {
    pub fn builder() -> MistralPromptBuilder {
        MistralPromptBuilder::default()
    }
    pub fn from_mistral_sender(sender: Sender<Request>) -> MistralPromptBuilder {
        Self::builder().sender(sender)
    }
}

impl Debug for MistralPrompt {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MistralPrompt").finish()
    }
}

#[async_trait]
impl SimplePrompt for MistralPrompt {
    async fn prompt(&self, prompt: &str) -> Result<String> {
        let (tx, mut rx) = channel(10_000);

        let request = Request::Normal(NormalRequest {
            messages: RequestMessage::Completion {
                text: prompt.to_string(),
                echo_prompt: false,
                best_of: 1,
            },
            sampling_params: SamplingParams::default(),
            response: tx,
            return_logprobs: false,
            is_streaming: false,
            id: 0,
            constraint: Constraint::None,
            suffix: None,
            adapters: None,
        });

        self.sender.send(request).await?;

        use Response::*;
        match rx.recv().await.context("No response for MistralPrompt")? {
            Done(response) => Ok(response.choices[0].message.content.clone()),
            InternalError(err) | ValidationError(err) => {
                anyhow::bail!(err)
            }
            ModelError(msg, _) => anyhow::bail!("Model error from Mistral prompt: {}", msg),
            _ => anyhow::bail!("Unexpected response from MistralPrompt"),
        }
    }
}
