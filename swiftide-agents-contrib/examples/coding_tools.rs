use std::sync::Arc;
use swiftide_agents::{Agent, DefaultContext};
use swiftide_agents_contrib::tools::{ApplyPatch, ExactReplace, PlanMemory, ReadFile, UpdatePlan};
use swiftide_core::chat_completion::{Tool, ToolCall};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let memory = Arc::new(PlanMemory::default());
    let plan = UpdatePlan::new(Arc::clone(&memory));
    let mut builder = Agent::builder();
    builder
        .add_tool(plan.clone())
        .add_tool(ReadFile::default())
        .add_tool(ExactReplace)
        .add_tool(ApplyPatch);
    // A real application supplies a model and an isolated executor before building the agent.
    // Invoke only the in-memory tool here; this offline example changes no files.
    let call = ToolCall::builder()
        .id("plan")
        .name("update_plan")
        .args(r#"{"items":[{"step":"Inspect the code","status":"in_progress"}]}"#)
        .build()?;
    println!("{}", plan.invoke(&DefaultContext::default(), &call).await?);
    println!("Application sees {} plan step(s)", memory.snapshot().len());
    Ok(())
}
