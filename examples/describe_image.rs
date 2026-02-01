//! Demonstrates passing an image to Chat Completions using a data URL.
//!
//! Set the `OPENAI_API_KEY` environment variable before running.

use anyhow::{Context as _, Result};
use base64::{Engine as _, engine::general_purpose};
use swiftide::chat_completion::{
    ChatCompletionRequest, ChatMessage, ChatMessageContentPart, ImageDetail,
};
use swiftide::traits::ChatCompletion;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_prompt_model("gpt-4o-mini")
        .build()?;

    let image_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../images/logo.png");
    let image_bytes = std::fs::read(&image_path).with_context(|| format!("Read {image_path:?}"))?;
    let encoded = general_purpose::STANDARD.encode(&image_bytes);
    let data_url = format!("data:image/png;base64,{encoded}");

    let message = ChatMessage::new_user_with_parts(vec![
        ChatMessageContentPart::text("Describe this image in one sentence."),
        ChatMessageContentPart::image_url(data_url, Some(ImageDetail::Auto)),
    ]);

    let request = ChatCompletionRequest::builder()
        .messages(vec![message])
        .build()?;

    let response = openai.complete(&request).await?;
    println!(
        "Image description: {}",
        response.message().unwrap_or("<no response>")
    );

    Ok(())
}
