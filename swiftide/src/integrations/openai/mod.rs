mod embed;
mod simple_prompt;

#[derive(Debug)]
pub struct OpenAI {
    client: async_openai::Client<async_openai::config::OpenAIConfig>,
    embed_model: String,
    prompt_model: String,
}
