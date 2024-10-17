#![allow(dead_code)]
use std::sync::Arc;

use anyhow::{anyhow, Result};
use swiftide_core::prompt::Prompt;

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
            .messages(self.context.message_history().await)
            .tools_spec(
                self.available_tools
                    .iter()
                    .map(|tool| tool.json_spec())
                    .collect::<Vec<_>>(),
            )
            .build()?;

        let response = self.llm.complete(chat_completion_request).await?;

        self.context.received_message(&response.message).await;

        // TODO: We can and should run tools in parallel or at least in a tokio spawn
        for (tool_name, tool_args) in response.tool_invocations {
            let tool = self
                .available_tools
                .iter()
                .find(|tool| tool.name() == tool_name)
                .ok_or_else(|| anyhow!("Tool {tool_name} not found"))?;

            let tool_output = tool
                .invoke(self.context.as_ref(), tool_args.as_deref())
                .await?;

            self.handle_tool_output(&tool_output).await?;
        }

        Ok(())
    }

    async fn handle_tool_output(&mut self, tool_output: &ToolOutput) -> Result<()> {
        match tool_output {
            ToolOutput::ToolCall(tool_call) => self.context.received_tool_call(&tool_call).await,
            ToolOutput::Stop(_) => self.should_stop = true,
        }

        Ok(())
    }
}
