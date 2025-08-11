//! Add tools provided by an MCP server to an agent
//!
//! Uses the `rmcp` crate to connect to an MCP server and list available tools, and invoke them
//!
//! Supports any transport that the `rmcp` crate supports
use std::borrow::Cow;
use std::{collections::HashMap, sync::Arc};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use rmcp::RoleClient;
use rmcp::model::{ClientInfo, Implementation, InitializeRequestParam};
use rmcp::service::RunningService;
use rmcp::transport::IntoTransport;
use rmcp::{ServiceExt, model::CallToolRequestParam};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use swiftide_core::CommandError;
use swiftide_core::chat_completion::ToolCall;
use swiftide_core::{
    Tool, ToolBox,
    chat_completion::{ParamSpec, ParamType, ToolSpec, errors::ToolError},
};
use tokio::sync::RwLock;

/// A filter to apply to the available tools
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToolFilter {
    Blacklist(Vec<String>),
    Whitelist(Vec<String>),
}

/// Connects to an MCP server and provides tools at runtime to the agent.
///
/// WARN: The rmcp has a quirky feature to serve from `()`. This does not work; serve from
/// `ClientInfo` instead, or from the transport and `Swiftide` will handle the rest.
#[derive(Clone)]
pub struct McpToolbox {
    service: Arc<RwLock<Option<RunningService<RoleClient, InitializeRequestParam>>>>,

    /// Optional human readable name for the toolbox
    name: Option<String>,

    filter: Arc<Option<ToolFilter>>,
}

impl McpToolbox {
    /// Blacklist tools by name, the agent will not be able to use these tools
    pub fn with_blacklist<ITEM: Into<String>, I: IntoIterator<Item = ITEM>>(
        &mut self,
        blacklist: I,
    ) -> &mut Self {
        let list = blacklist.into_iter().map(Into::into).collect::<Vec<_>>();
        self.filter = Some(ToolFilter::Blacklist(list)).into();
        self
    }

    /// Whitelist tools by name, the agent will only be able to use these tools
    pub fn with_whitelist<ITEM: Into<String>, I: IntoIterator<Item = ITEM>>(
        &mut self,
        blacklist: I,
    ) -> &mut Self {
        let list = blacklist.into_iter().map(Into::into).collect::<Vec<_>>();
        self.filter = Some(ToolFilter::Whitelist(list)).into();
        self
    }

    /// Apply a custom filter to the tools
    pub fn with_filter(&mut self, filter: ToolFilter) -> &mut Self {
        self.filter = Some(filter).into();
        self
    }

    /// Apply an optional name to the toolbox
    pub fn with_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = Some(name.into());
        self
    }

    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("MCP Toolbox")
    }

    /// Create a new toolbox from a transport
    ///
    /// # Errors
    ///
    /// Errors if the transport fails to connect
    pub async fn try_from_transport<
        E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
        A,
    >(
        transport: impl IntoTransport<RoleClient, E, A>,
    ) -> Result<Self> {
        let info = Self::default_client_info();
        let service = Arc::new(RwLock::new(Some(info.serve(transport).await?)));

        Ok(Self {
            service,
            filter: None.into(),
            name: None,
        })
    }

    /// Create a new toolbox from a running service
    pub fn from_running_service(
        service: RunningService<RoleClient, InitializeRequestParam>,
    ) -> Self {
        Self {
            service: Arc::new(RwLock::new(Some(service))),
            filter: None.into(),
            name: None,
        }
    }

    fn default_client_info() -> ClientInfo {
        ClientInfo {
            client_info: Implementation {
                name: "swiftide".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            ..Default::default()
        }
    }

    /// Disconnects from the MCP server if it is running
    ///
    /// If it is not running, an Ok is returned and it logs a tracing message
    ///
    /// # Errors
    ///
    /// Errors if the service is running but cannot be stopped
    pub async fn cancel(&mut self) -> Result<()> {
        let mut lock = self.service.write().await;
        let Some(service) = std::mem::take(&mut *lock) else {
            tracing::warn!("mcp server is not running");
            return Ok(());
        };

        tracing::debug!(name = self.name(), "Stopping mcp server");

        service
            .cancel()
            .await
            .context("failed to stop mcp server")?;

        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct ToolInputSchema {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub type_: String, // This _must_ be object
    pub properties: Option<HashMap<String, Value>>,
    pub required: Option<Vec<String>>,
}

#[async_trait]
impl ToolBox for McpToolbox {
    #[tracing::instrument(skip_all)]
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        let Some(service) = &*self.service.read().await else {
            anyhow::bail!("No service available");
        };
        tracing::debug!(name = self.name(), "Connecting to mcp server");
        let peer_info = service.peer_info();
        tracing::debug!(?peer_info, name = self.name(), "Connected to mcp server");

        tracing::debug!(name = self.name(), "Listing tools from mcp server");
        let tools = service
            .list_all_tools()
            .await
            .context("Failed to list tools")?;

        let tools = tools
            .into_iter()
            .map(|t| {
                let schema: ToolInputSchema = serde_json::from_value(t.schema_as_json_value())
                    .context("Failed to parse tool input schema")?;

                tracing::trace!(?schema, "Parsing tool input schema for {}", t.name);

                let mut tool_spec = ToolSpec::builder()
                    .name(t.name.clone())
                    .description(t.description.unwrap_or_default())
                    .to_owned();
                let mut parameters = Vec::new();

                if let Some(mut p) = schema.properties {
                    for (name, value) in &mut p {
                        let param = ParamSpec::builder()
                            .name(name)
                            .description(
                                value
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .unwrap_or(""),
                            )
                            .ty(value
                                .get_mut("type")
                                .and_then(|t| serde_json::from_value(t.take()).ok())
                                .unwrap_or(ParamType::String))
                            .required(schema.required.as_ref().is_some_and(|r| r.contains(name)))
                            .build()
                            .context("Failed to build parameters for mcp tool")?;

                        parameters.push(param);
                    }
                }

                tool_spec.parameters(parameters);
                let tool_spec = tool_spec.build().context("Failed to build tool spec")?;

                Ok(Box::new(McpTool {
                    client: Arc::clone(&self.service),
                    tool_name: t.name.into(),
                    tool_spec,
                }) as Box<dyn Tool>)
            })
            .collect::<Result<Vec<_>>>()
            .context("Failed to build mcp tool specs")?;

        if let Some(filter) = self.filter.as_ref() {
            match filter {
                ToolFilter::Blacklist(blacklist) => {
                    let blacklist = blacklist.iter().map(String::as_str).collect::<Vec<_>>();
                    Ok(tools
                        .into_iter()
                        .filter(|t| !blacklist.contains(&t.name().as_ref()))
                        .collect())
                }
                ToolFilter::Whitelist(whitelist) => {
                    let whitelist = whitelist.iter().map(String::as_str).collect::<Vec<_>>();
                    Ok(tools
                        .into_iter()
                        .filter(|t| whitelist.contains(&t.name().as_ref()))
                        .collect())
                }
            }
        } else {
            Ok(tools)
        }
    }

    fn name(&self) -> Cow<'_, str> {
        self.name().into()
    }
}

#[derive(Clone)]
struct McpTool {
    client: Arc<RwLock<Option<RunningService<RoleClient, InitializeRequestParam>>>>,
    tool_name: String,
    tool_spec: ToolSpec,
}

#[async_trait]
impl Tool for McpTool {
    async fn invoke(
        &self,
        _agent_context: &dyn swiftide_core::AgentContext,
        tool_call: &ToolCall,
    ) -> Result<
        swiftide_core::chat_completion::ToolOutput,
        swiftide_core::chat_completion::errors::ToolError,
    > {
        let args = match tool_call.args() {
            Some(args) => Some(serde_json::from_str(args).map_err(ToolError::WrongArguments)?),
            None => None,
        };

        let request = CallToolRequestParam {
            name: self.tool_name.clone().into(),
            arguments: args,
        };

        let Some(service) = &*self.client.read().await else {
            return Err(
                CommandError::ExecutorError(anyhow::anyhow!("mcp server is not running")).into(),
            );
        };

        tracing::debug!(request = ?request, tool = self.name().as_ref(), "Invoking mcp tool");
        let response = service
            .call_tool(request)
            .await
            .context("Failed to call tool")?;

        tracing::debug!(response = ?response, tool = self.name().as_ref(), "Received response from mcp tool");
        let Some(content) = response.content else {
            if response.is_error.unwrap_or(false) {
                return Err(ToolError::Unknown(anyhow::anyhow!(
                    "Error received from mcp tool without content"
                )));
            }

            return Ok("Tool executed successfully".into());
        };
        let content = content
            .into_iter()
            .filter_map(|c| c.as_text().map(|t| t.text.to_string()))
            .collect::<Vec<_>>()
            .join("\n");

        if let Some(error) = response.is_error
            && error
        {
            return Err(ToolError::Unknown(anyhow::anyhow!(
                "Failed to execute mcp tool: {content}"
            )));
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

#[cfg(test)]
mod tests {
    use super::*;
    use copied_from_rmcp::Calculator;
    use rmcp::serve_server;
    use serde_json::json;
    use tokio::net::{UnixListener, UnixStream};

    const SOCKET_PATH: &str = "/tmp/swiftide-mcp.sock";

    #[allow(clippy::similar_names)]
    #[test_log::test(tokio::test(flavor = "multi_thread"))]
    async fn test_socket() {
        let _ = std::fs::remove_file(SOCKET_PATH);

        match UnixListener::bind(SOCKET_PATH) {
            Ok(unix_listener) => {
                println!("Server successfully listening on {SOCKET_PATH}");
                tokio::spawn(server(unix_listener));
            }
            Err(e) => {
                println!("Unable to bind to {SOCKET_PATH}: {e}");
            }
        }

        let client = client().await.unwrap();

        let t = client.available_tools().await.unwrap();
        assert_eq!(client.available_tools().await.unwrap().len(), 3);

        let mut names = t.iter().map(|t| t.name()).collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["optional", "sub", "sum"]);

        let sum_tool = t.iter().find(|t| t.name() == "sum").unwrap();
        let mut builder = ToolCall::builder()
            .id("some")
            .args(r#"{"b": "hello"}"#)
            .name("test")
            .name("test")
            .to_owned();

        assert_eq!(sum_tool.tool_spec().name, "sum");

        let tool_call = builder.args(r#"{"a": 10, "b": 20}"#).build().unwrap();

        let result = sum_tool
            .invoke(&(), &tool_call)
            .await
            .unwrap()
            .content()
            .unwrap()
            .to_string();
        assert_eq!(result, "30");

        let sub_tool = t.iter().find(|t| t.name() == "sub").unwrap();
        assert_eq!(sub_tool.tool_spec().name, "sub");

        let tool_call = builder.args(r#"{"a": 10, "b": 20}"#).build().unwrap();

        let result = sub_tool
            .invoke(&(), &tool_call)
            .await
            .unwrap()
            .content()
            .unwrap()
            .to_string();
        assert_eq!(result, "-10");

        // The input schema type for the input param is ["string", "null"]
        let optional_tool = t.iter().find(|t| t.name() == "optional").unwrap();
        dbg!(optional_tool.tool_spec());
        assert_eq!(optional_tool.tool_spec().name, "optional");
        assert_eq!(optional_tool.tool_spec().parameters.len(), 1);
        assert_eq!(
            serde_json::to_string(&optional_tool.tool_spec().parameters[0].ty).unwrap(),
            json!("string").to_string()
        );

        let tool_call = builder.args(r#"{"text": "hello"}"#).build().unwrap();

        let result = optional_tool
            .invoke(&(), &tool_call)
            .await
            .unwrap()
            .content()
            .unwrap()
            .to_string();
        assert_eq!(result, "hello");

        let tool_call = builder.args(r#"{"text": null}"#).build().unwrap();
        let result = optional_tool
            .invoke(&(), &tool_call)
            .await
            .unwrap()
            .content()
            .unwrap()
            .to_string();
        assert_eq!(result, "");

        // Clean up socket file
        let _ = std::fs::remove_file(SOCKET_PATH);
    }

    async fn server(unix_listener: UnixListener) -> anyhow::Result<()> {
        while let Ok((stream, addr)) = unix_listener.accept().await {
            println!("Client connected: {addr:?}");
            tokio::spawn(async move {
                match serve_server(Calculator::new(), stream).await {
                    Ok(server) => {
                        println!("Server initialized successfully");
                        if let Err(e) = server.waiting().await {
                            println!("Error while server waiting: {e:?}");
                        }
                    }
                    Err(e) => println!("Server initialization failed: {e:?}"),
                }

                anyhow::Ok(())
            });
        }
        Ok(())
    }

    async fn client() -> anyhow::Result<McpToolbox> {
        println!("Client connecting to {SOCKET_PATH}");
        let stream = UnixStream::connect(SOCKET_PATH).await?;

        // let client = serve_client((), stream).await?;
        let client = McpToolbox::try_from_transport(stream).await?;
        println!("Client connected and initialized successfully");

        Ok(client)
    }

    #[allow(clippy::unused_self)]
    mod copied_from_rmcp {
        use rmcp::{
            ErrorData as McpError, ServerHandler,
            handler::server::tool::{Parameters, ToolRouter},
            model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
            schemars, tool, tool_handler,
        };

        #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
        pub struct Request {
            pub a: i32,
            pub b: i32,
        }

        #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
        pub struct OptRequest {
            pub text: Option<String>,
        }

        #[derive(Debug, Clone)]
        pub struct Calculator {
            tool_router: ToolRouter<Self>,
        }

        #[rmcp::tool_router]
        impl Calculator {
            pub fn new() -> Self {
                Self {
                    tool_router: Self::tool_router(),
                }
            }

            #[allow(clippy::unnecessary_wraps)]
            #[tool(description = "Calculate the sum of two numbers")]
            fn sum(
                &self,
                Parameters(Request { a, b }): Parameters<Request>,
            ) -> Result<CallToolResult, McpError> {
                Ok(CallToolResult::success(vec![Content::text(
                    (a + b).to_string(),
                )]))
            }

            #[allow(clippy::unnecessary_wraps)]
            #[tool(description = "Calculate the sum of two numbers")]
            fn sub(
                &self,
                Parameters(Request { a, b }): Parameters<Request>,
            ) -> Result<CallToolResult, McpError> {
                Ok(CallToolResult::success(vec![Content::text(
                    (a - b).to_string(),
                )]))
            }

            #[allow(clippy::unnecessary_wraps)]
            #[tool(description = "Optional echo")]
            fn optional(
                &self,
                Parameters(OptRequest { text }): Parameters<OptRequest>,
            ) -> Result<CallToolResult, McpError> {
                Ok(CallToolResult::success(vec![Content::text(
                    text.unwrap_or_default(),
                )]))
            }
        }

        #[tool_handler]
        impl ServerHandler for Calculator {
            fn get_info(&self) -> ServerInfo {
                ServerInfo {
                    instructions: Some("A simple calculator".into()),
                    capabilities: ServerCapabilities::builder().enable_tools().build(),
                    ..Default::default()
                }
            }
        }
    }
}
