use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use mcp_client::{transport::Error, McpClientTrait as _, Transport};
use mcp_spec::{protocol::JsonRpcMessage, Content};
use serde::Deserialize;
use serde_json::{json, Value};
use swiftide_core::{
    chat_completion::{errors::ToolError, ParamSpec, ParamType, ToolSpec},
    Tool, ToolBox,
};
use tower_service::Service;

#[derive(Clone)]
struct McpClient<S>
where
    S: Service<JsonRpcMessage, Response = JsonRpcMessage> + Clone + Send + Sync + 'static,
    S::Error: Into<Error>,
    S::Future: Send,
    mcp_client::Error: std::convert::From<
        <S as tower_service::Service<mcp_spec::protocol::JsonRpcMessage>>::Error,
    >,
{
    client: Arc<mcp_client::client::McpClient<S>>,
}

impl<S> McpClient<S>
where
    S: Service<JsonRpcMessage, Response = JsonRpcMessage> + Clone + Send + Sync + 'static,
    S::Error: Into<Error>,
    S::Future: Send,
    mcp_client::Error: std::convert::From<
        <S as tower_service::Service<mcp_spec::protocol::JsonRpcMessage>>::Error,
    >,
{
    pub fn from_client(client: impl Into<Arc<mcp_client::client::McpClient<S>>>) -> Self {
        Self {
            client: client.into(),
        }
    }
}

// wtf nice schema bro
#[derive(Deserialize)]
struct ToolInputSchema {
    pub type_: String,
    pub properties: Option<HashMap<String, Value>>,
    pub required: Option<Vec<String>>,
}

#[async_trait]
impl<S> ToolBox for McpClient<S>
where
    S: Service<JsonRpcMessage, Response = JsonRpcMessage> + Clone + Send + Sync + 'static,
    S::Error: Into<Error>,
    S::Future: Send,
    mcp_client::Error: std::convert::From<
        <S as tower_service::Service<mcp_spec::protocol::JsonRpcMessage>>::Error,
    >,
{
    #[tracing::instrument(skip_all)]
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        let tools = match self.client.list_tools(None).await {
            Ok(tools) => tools,
            Err(e) => {
                // Probably should return the error instead but it is late
                tracing::error!("Failed to list tools: {:?}", e);
                return Ok(vec![]);
            }
        };

        tools
            .tools
            .into_iter()
            .map(|t| {
                let schema: ToolInputSchema = serde_json::from_value(t.input_schema)
                    .context("Failed to parse tool input schema")?;

                let mut tool_spec = ToolSpec::builder()
                    .name(t.name.clone())
                    .description(t.description)
                    .to_owned();
                let mut parameters = Vec::new();

                if let Some(p) = schema.properties {
                    let param = ParamSpec::builder()
                        .name(
                            p.get("name")
                                .and_then(serde_json::Value::as_str)
                                .context("No name for param")?,
                        )
                        .description(
                            p.get("description")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default(),
                        )
                        .ty(p.get("type").map_or(ParamType::String, |t| {
                            serde_json::from_value(t.clone()).unwrap_or(ParamType::String)
                        }))
                        .build()
                        .context("Failed to build parameters")?;

                    parameters.push(param);
                }

                tool_spec.parameters(parameters);
                let tool_spec = tool_spec.build().context("Failed to build tool spec")?;

                Ok(Box::new(McpTool {
                    client: self.client.clone(),
                    tool_name: t.name.clone(),
                    tool_spec,
                }) as Box<dyn Tool>)
            })
            .collect::<Result<Vec<_>>>()
    }
}

#[derive(Clone)]
struct McpTool<S>
where
    S: Service<JsonRpcMessage, Response = JsonRpcMessage> + Clone + Send + Sync + 'static,
    S::Error: Into<Error>,
    S::Future: Send,
    mcp_client::Error: std::convert::From<
        <S as tower_service::Service<mcp_spec::protocol::JsonRpcMessage>>::Error,
    >,
{
    client: Arc<mcp_client::client::McpClient<S>>,
    tool_name: String,
    tool_spec: ToolSpec,
}

#[async_trait]
impl<S> Tool for McpTool<S>
where
    S: Service<JsonRpcMessage, Response = JsonRpcMessage> + Clone + Send + Sync + 'static,
    S::Error: Into<Error>,
    S::Future: Send,
    mcp_client::Error: std::convert::From<
        <S as tower_service::Service<mcp_spec::protocol::JsonRpcMessage>>::Error,
    >,
{
    async fn invoke(
        &self,
        _agent_context: &dyn swiftide_core::AgentContext,
        raw_args: Option<&str>,
    ) -> Result<
        swiftide_core::chat_completion::ToolOutput,
        swiftide_core::chat_completion::errors::ToolError,
    > {
        let response = self
            .client
            .call_tool(&self.tool_name, serde_json::to_value(raw_args)?)
            .await
            .context("Failed to call mcp tool")?;

        let content = response
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("\n");

        if let Some(error) = response.is_error {
            if error {
                ToolError::Unknown(anyhow::anyhow!("Failed to execute mcp tool: {content}"));
            }
        }

        Ok(content.into())
    }

    fn name(&self) -> std::borrow::Cow<'_, str> {
        self.tool_name.as_str().into()
    }

    fn tool_spec(&self) -> ToolSpec {
        self.tool_spec.clone()
    }
}
