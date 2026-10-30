use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use swiftide_agents::Agent;
use swiftide_core::{
    AgentContext,
    chat_completion::{ChatMessage, Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
};

#[derive(Clone, Debug)]
struct ReadOriginalInstructions {
    instructions: String,
}

impl ReadOriginalInstructions {
    fn new(instructions: impl Into<String>) -> Self {
        Self {
            instructions: instructions.into(),
        }
    }
}

impl ReadOriginalInstructions {
    fn read_original_instructions(
        &self,
        _agent_context: &dyn AgentContext,
    ) -> impl Future<Output = Result<ToolOutput, ToolError>> {
        std::future::ready(Ok(ToolOutput::text(self.instructions.clone())))
    }
}

pub(in crate::hooks) async fn install(agent: &mut Agent) -> Result<()> {
    let history = agent.context().history().await?;
    let Some(original_instructions) = original_user_message(&history) else {
        tracing::warn!(
            "Could not install original instructions recovery tool: no user message found"
        );
        return Ok(());
    };

    agent.add_tool(Box::new(ReadOriginalInstructions::new(
        original_instructions,
    )));

    Ok(())
}

fn original_user_message(history: &[ChatMessage]) -> Option<String> {
    history.iter().find_map(|message| match message {
        ChatMessage::User(message) => Some(message.clone()),
        _ => None,
    })
}

#[async_trait]
impl Tool for ReadOriginalInstructions {
    async fn invoke(
        &self,
        context: &dyn AgentContext,
        _call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        self.read_original_instructions(context).await
    }
    fn name(&self) -> Cow<'_, str> {
        "read_original_instructions".into()
    }
    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder().name("read_original_instructions")
            .description("Read the original instructions that started this agent step after conversation compaction. Read this before completing your task.")
            .build().expect("valid recovery tool")
    }
}
