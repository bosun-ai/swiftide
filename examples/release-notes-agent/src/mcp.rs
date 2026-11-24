//! Optional GitHub tools from the GitHub MCP server.
use std::{borrow::Cow, env, process::Stdio, time::Duration};

use anyhow::{Context as _, Result, bail};
use async_trait::async_trait;
use rmcp::transport::{ConfigureCommandExt as _, TokioChildProcess};
use swiftide::{
    agents::tools::mcp::McpToolbox,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
    traits::{AgentContext, ToolBox},
};
use tokio::{process::Command, time::timeout};

/// Read-only tools the agent may use to look up the pull requests behind commits.
const GITHUB_TOOLS: [&str; 2] = ["get_pull_request", "get_issue"];

// ANCHOR: mcp
/// Starts `@modelcontextprotocol/server-github` with `npx` and exposes a few of its tools.
///
/// Errors explain what is missing, so the caller can carry on without GitHub.
pub async fn github_toolbox() -> Result<impl ToolBox> {
    let token = env::var("GITHUB_PERSONAL_ACCESS_TOKEN")
        .or_else(|_| env::var("GITHUB_TOKEN"))
        .context("set GITHUB_PERSONAL_ACCESS_TOKEN (or GITHUB_TOKEN)")?;

    let server = Command::new("npx").configure(|cmd| {
        cmd.args(["-y", "@modelcontextprotocol/server-github"])
            .env("GITHUB_PERSONAL_ACCESS_TOKEN", token);
    });
    let (transport, _stderr) = TokioChildProcess::builder(server)
        .stderr(Stdio::null())
        .spawn()
        .context("could not run `npx`; is Node.js installed?")?;

    let mut toolbox = timeout(
        Duration::from_mins(1),
        McpToolbox::try_from_transport(transport),
    )
    .await
    .context("the GitHub MCP server did not start within a minute")?
    .context("the GitHub MCP server failed to start")?;
    toolbox.with_whitelist(GITHUB_TOOLS);
    // Swiftide names MCP tools `<server>:<tool>`; OpenAI rejects ':', see `OpenAiToolNames` below.
    let toolbox = OpenAiToolNames(toolbox);

    // Fail early, rather than mid-run, if the server does not offer the tools we expect.
    let available = toolbox.available_tools().await?;
    if available.len() != GITHUB_TOOLS.len() {
        bail!("the GitHub MCP server does not offer {GITHUB_TOOLS:?}");
    }
    Ok(toolbox)
}
// ANCHOR_END: mcp

/// Swiftide registers MCP tools as `<server name>:<tool>`, but `OpenAI` only accepts tool names
/// matching `^[a-zA-Z0-9_-]+$`. This toolbox replaces anything else with `_`.
#[derive(Clone)]
struct OpenAiToolNames<T>(T);

#[async_trait]
impl<T: ToolBox + Clone> ToolBox for OpenAiToolNames<T> {
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        let tools = self.0.available_tools().await?;
        Ok(tools
            .into_iter()
            .map(|tool| Box::new(RenamedTool::new(tool)) as Box<dyn Tool>)
            .collect())
    }

    fn name(&self) -> Cow<'_, str> {
        self.0.name()
    }
}

#[derive(Clone)]
struct RenamedTool {
    name: String,
    inner: Box<dyn Tool>,
}

impl RenamedTool {
    fn new(inner: Box<dyn Tool>) -> Self {
        let name = inner
            .name()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Self { name, inner }
    }
}

#[async_trait]
impl Tool for RenamedTool {
    async fn invoke(
        &self,
        context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        self.inner.invoke(context, tool_call).await
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.name)
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.clone(),
            ..self.inner.tool_spec()
        }
    }
}
