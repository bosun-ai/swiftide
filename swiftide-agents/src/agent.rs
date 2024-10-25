#![allow(dead_code)]
use crate::{agent_context::DefaultContext, tools::control::Stop};
use std::collections::HashSet;

use anyhow::Result;
use derive_builder::Builder;
use swiftide_core::{
    chat_completion::{ChatCompletion, ChatCompletionRequest, ChatMessage, ToolCall, ToolOutput},
    prompt::Prompt,
};
use tracing::debug;

use crate::traits::*;

// Notes
//
// Generic over LLM instead of box dyn? Should tool support be a separate trait?
#[derive(Clone, Builder)]
pub struct Agent {
    // name: String,
    #[builder(default = "Box::new(DefaultContext::default())")]
    context: Box<dyn AgentContext>,
    #[builder(setter(into))]
    instructions: Prompt,
    #[builder(default = "Agent::default_tools()", setter(custom))]
    tools: HashSet<Box<dyn Tool>>,

    #[builder(setter(custom))]
    llm: Box<dyn ChatCompletion>,

    #[builder(private, default)]
    should_stop: bool,
}

impl AgentBuilder {
    pub fn llm<LLM: ChatCompletion + Clone + 'static>(&mut self, llm: &LLM) -> &mut Self {
        let boxed: Box<dyn ChatCompletion> = llm.clone().into();

        self.llm = Some(boxed);
        self
    }

    pub fn tools<TOOL: Into<Box<dyn Tool>>, I: IntoIterator<Item = TOOL>>(
        &mut self,
        tools: I,
    ) -> &mut Self {
        self.tools = Some(
            tools
                .into_iter()
                .map(Into::into)
                .chain(Agent::default_tools())
                .collect(),
        );
        self
    }
}

impl Agent {
    pub fn builder() -> AgentBuilder {
        AgentBuilder::default()
    }

    fn default_tools() -> HashSet<Box<dyn Tool>> {
        HashSet::from([Box::new(Stop::default()) as Box<dyn Tool>])
    }

    pub async fn run(&mut self) -> Result<()> {
        self.context
            .record_in_history(ChatMessage::User(self.instructions.render().await?))
            .await;

        // LIFECYCLE: BEFORE ALL
        while !self.should_stop {
            debug!("Looping agent");
            // LIFECYCLE: BEFORE EACH
            self.run_once().await?;
            // LIFECYCLE: AFTER TOOL
            // LIFECYCLE: AFTER EACH
        }

        Ok(())
        // LIFECYCLE: AFTER ALL
    }

    async fn run_once(&mut self) -> Result<()> {
        // TODO: Since control flow is now via tools, tools should always include them
        let chat_completion_request = ChatCompletionRequest::builder()
            .messages(self.context.conversation_history().await)
            .tools_spec(
                self.tools
                    .iter()
                    .map(|tool| tool.json_spec())
                    .collect::<HashSet<_>>(),
            )
            .build()?;

        debug!("Calling LLM with request: {:?}", chat_completion_request);
        let response = self.llm.complete(&chat_completion_request).await?;

        if let Some(message) = response.message {
            debug!("LLM returned message: {}", message);
            self.context
                .record_in_history(ChatMessage::Assistant(message))
                .await;
        }

        // TODO: We can and should run tools in parallel or at least in a tokio spawn
        if let Some(tool_calls) = response.tool_calls {
            debug!("LLM returned tool calls: {:?}", tool_calls);
            for tool_call in tool_calls {
                let Some(tool) = self.find_tool_by_name(tool_call.name()) else {
                    tracing::warn!("Tool {} not found", tool_call.name());
                    continue;
                };
                let output = tool.invoke(&self.context, tool_call.args()).await?;

                self.handle_control_tools(&output);

                self.context
                    .record_in_history(ChatMessage::ToolOutput(tool_call, output))
                    .await;
            }
        }

        Ok(())
    }

    fn find_tool_by_name(&self, tool_name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|tool| tool.name() == tool_name)
            .map(|boxed| &**boxed)
    }

    /// Handle any tool specific output (e.g. stop)
    fn handle_control_tools(&mut self, output: &ToolOutput) {
        if let ToolOutput::Stop = output {
            self.should_stop = true;
        }
    }
}

impl AgentBuilder {}

#[cfg(test)]
mod tests {

    use swiftide_core::chat_completion::ChatCompletionResponse;
    use swiftide_core::test_utils::MockChatCompletion;

    use super::*;

    use crate::test_utils::MockTool;

    #[test_log::test(tokio::test)]
    async fn test_agent_builder_defaults() {
        // Create a prompt
        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();

        // Build the agent
        let agent = Agent::builder()
            .instructions(prompt)
            .llm(&mock_llm)
            .build()
            .unwrap();

        // Check that the context is the default context

        // Check that the default tools are added
        assert!(agent.find_tool_by_name("stop").is_some());

        // Check it does not allow duplicates
        let agent = Agent::builder()
            .instructions(prompt)
            .tools([Stop::default(), Stop::default()])
            .llm(&mock_llm)
            .build()
            .unwrap();

        assert_eq!(agent.tools.len(), 1);

        // It should include the default tool if a different tool is provided
        let agent = Agent::builder()
            .instructions(prompt)
            .tools([MockTool::new()])
            .llm(&mock_llm)
            .build()
            .unwrap();

        assert_eq!(agent.tools.len(), 2);
        assert!(agent.find_tool_by_name("mock_tool").is_some());
        assert!(agent.find_tool_by_name("stop").is_some());
    }

    #[test_log::test(tokio::test)]
    async fn test_agent_tool_calling_loop() {
        let prompt = "Write a poem";
        let mock_llm = MockChatCompletion::new();
        let mock_tool = MockTool::new();

        // TODO: Write better expectation management on agent loop
        // i.e. assertion object on agent itself
        let chat_request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("Write a poem".to_string())])
            .tools_spec(
                [Box::new(mock_tool.clone()) as Box<dyn Tool>]
                    .into_iter()
                    .chain(Agent::default_tools())
                    .map(|tool| tool.json_spec())
                    .collect::<HashSet<_>>(),
            )
            .build()
            .unwrap();

        let mock_tool_response = ChatCompletionResponse::builder()
            .message("Roses are red".to_string())
            .tool_calls(vec![ToolCall::builder()
                .name("mock_tool")
                .id("1")
                .build()
                .unwrap()])
            .build()
            .unwrap();

        mock_llm.expect_complete(chat_request.clone(), Ok(mock_tool_response));

        let chat_request = ChatCompletionRequest::builder()
            .messages(vec![
                ChatMessage::User("Write a poem".to_string()),
                ChatMessage::Assistant("Roses are red".to_string()),
                ChatMessage::ToolOutput(
                    ToolCall::builder()
                        .name("mock_tool")
                        .id("1")
                        .build()
                        .unwrap(),
                    ToolOutput::Content("Great!".to_string()),
                ),
            ])
            .tools_spec(
                [Box::new(mock_tool.clone()) as Box<dyn Tool>]
                    .into_iter()
                    .chain(Agent::default_tools())
                    .map(|tool| tool.json_spec())
                    .collect::<HashSet<_>>(),
            )
            .build()
            .unwrap();

        let stop_response = ChatCompletionResponse::builder()
            .message("Roses are red".to_string())
            .tool_calls(vec![ToolCall::builder()
                .name("stop")
                .id("1")
                .build()
                .unwrap()])
            .build()
            .unwrap();

        mock_llm.expect_complete(chat_request, Ok(stop_response));
        mock_tool.expect_invoke("Great!".into(), None);

        let mut agent = Agent::builder()
            .instructions(prompt)
            .tools([mock_tool])
            .llm(&mock_llm)
            .build()
            .unwrap();

        agent.run().await.unwrap();
    }
}
