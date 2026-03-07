use config::DashscopeConfig;

use crate::openai;

mod config;

pub type Dashscope = openai::GenericOpenAI<DashscopeConfig>;
impl Dashscope {
    pub fn builder() -> DashscopeBuilder {
        DashscopeBuilder::default()
    }
}

pub type DashscopeBuilder = openai::GenericOpenAIBuilder<DashscopeConfig>;
pub type DashscopeBuilderError = openai::GenericOpenAIBuilderError;
pub use openai::{Options, OptionsBuilder, OptionsBuilderError};

impl Default for Dashscope {
    fn default() -> Self {
        Dashscope::builder().build().unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_default_prompt_model() {
        let openai = Dashscope::builder()
            .default_prompt_model("qwen-long")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some("qwen-long".to_string())
        );

        let openai = Dashscope::builder()
            .default_prompt_model("qwen-turbo")
            .build()
            .unwrap();
        assert_eq!(
            openai.default_options.prompt_model,
            Some("qwen-turbo".to_string())
        );
    }
}
