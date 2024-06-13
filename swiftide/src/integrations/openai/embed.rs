/// This module provides the implementation for embedding input data using the OpenAI API.
/// It is a part of the Swiftide project, which is an asynchronous file ingestion and processing system designed for use in a Research Augmented Generation (RAG) system.
/// This module is essential for converting text data into embeddings, which are then utilized for various tasks such as natural language processing, similarity computation, or machine learning.
use anyhow::{Context as _, Result};
use async_openai::types::CreateEmbeddingRequestArgs;
use async_trait::async_trait;

use crate::{Embed, Embeddings};

use super::OpenAI;

/// Implementation of the `Embed` trait for the `OpenAI` struct.
/// This implementation provides the functionality to generate embeddings for a given input using the OpenAI API.
#[async_trait]
impl Embed for OpenAI {
    /// Generates embeddings for the given input using the OpenAI API.
    ///
    /// # Parameters
    ///
    /// - `input`: A vector of strings representing the input data to be embedded.
    ///
    /// # Returns
    ///
    /// - `Result<Embeddings>`: A result containing the embeddings if successful, or an error if the operation fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the model is not set, if there is an issue building the request,
    /// or if the API call to OpenAI fails.
    ///
    /// # Performance
    ///
    /// This function performs an asynchronous API call to OpenAI, which may introduce latency.
    /// It also assumes that the order of the input is preserved in the response, which might not always be the case.
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings> {
        let model = self
            .default_options
            .embed_model
            .as_ref()
            .context("Model not set")?;

        let request = CreateEmbeddingRequestArgs::default()
            .model(model)
            .input(input)
            .build()?;
        tracing::debug!(
            messages = serde_json::to_string_pretty(&request)?,
            "[Embed] Request to openai"
        );
        let response = self.client.embeddings().create(request).await?;
        tracing::debug!("[Embed] Response openai");

        // WARN: Naively assumes that the order is preserved. Might not always be the case.
        Ok(response.data.into_iter().map(|d| d.embedding).collect())
    }
}
