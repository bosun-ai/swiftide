#![allow(dead_code)]
use std::sync::Arc;

use anyhow::{anyhow, Result};
use swiftide_core::{
    chat_completion::{ChatCompletion, ChatCompletionRequest, ChatMessage, ToolCall, ToolOutput},
    prompt::Prompt,
};

use crate::traits::*;

// Notes
//
// Generic over LLM instead of box dyn? Should tool support be a separate trait?
pub struct Agent {
    name: String,
    context: Box<dyn AgentContext>,
    instructions: Prompt,
    available_tools: Vec<Box<dyn Tool>>,
    llm: Box<dyn ChatCompletion>,

    should_stop: bool,
}

impl Agent {
    pub async fn run(&mut self) -> Result<()> {
        // LIFECYCLE: BEFORE ALL
        while !self.should_stop {
            // LIFECYCLE: BEFORE EACH
            self.run_once().await?;
            // LIFECYCLE: AFTER TOOL
            // LIFECYCLE: AFTER EACH
        }

        Ok(())
        // LIFECYCLE: AFTER ALL
    }

    pub async fn run_once(&mut self) -> Result<()> {
        // TODO: Since control flow is now via tools, tools should always include them
        let chat_completion_request = ChatCompletionRequest::builder()
            .system_prompt(&self.instructions)
            .messages(self.context.conversation_history().await)
            .tools_spec(
                self.available_tools
                    .iter()
                    .map(|tool| tool.json_spec())
                    .collect::<Vec<_>>(),
            )
            .build()?;

        let response = self.llm.complete(&chat_completion_request).await?;

        if let Some(message) = response.message {
            self.context
                .record_in_history(ChatMessage::Assistant(message))
                .await;
        }

        // TODO: We can and should run tools in parallel or at least in a tokio spawn
        if let Some(tool_calls) = response.tool_calls {
            for tool_call in tool_calls {
                let tool_output = self.call_tool(&tool_call).await?;

                self.handle_tool_output(&tool_output).await?;

                self.context
                    .record_in_history(ChatMessage::ToolOutput(tool_output))
                    .await;
            }
        }

        Ok(())
    }

    // Calls a tool by name and returns the output
    //
    // Errors if the tool can not be found
    async fn call_tool(&self, tool_call: &ToolCall) -> Result<ToolOutput> {
        let tool = self
            .available_tools
            .iter()
            .find(|tool| tool.name() == tool_call.name())
            .ok_or_else(|| anyhow!("Tool not found"))?;

        tool.invoke(&self.context, tool_call.args().as_deref())
            .await
    }

    /// Handle any tool specific output (e.g. stop)
    async fn handle_tool_output(&mut self, tool_output: &ToolOutput) -> Result<()> {
        match tool_output {
            ToolOutput::Stop(_) => self.should_stop = true,
            _ => (),
        }

        Ok(())
    }
}
