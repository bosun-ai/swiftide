//! Add tools provided by an MCP server to an agent
//!
//! Uses the `rmcp` crate to connect to an MCP server and list available tools, and invoke them
//!
//! Supports any transport that the `rmcp` crate supports
use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use derive_builder::Builder;
use rmcp::model::{ClientCapabilities, ClientInfo, Implementation, InitializeRequestParam};
use rmcp::service::RunningService;
use rmcp::transport::IntoTransport;
use rmcp::{model::CallToolRequestParam, ServiceExt};
use rmcp::{RoleClient, Service};
use serde::Deserialize;
use serde_json::{json, Value};
use swiftide_core::{
    chat_completion::{errors::ToolError, ParamSpec, ParamType, ToolSpec},
    Tool, ToolBox,
};

enum Filter {
    Blacklist(Vec<String>),
    Whitelist(Vec<String>),
}

/// A client for an MCP server
///
/// # Example
///
/// ```no_run
///
///
///
/// ```
#[derive(Clone)]
pub struct McpClient {
    client: Arc<RunningService<RoleClient, InitializeRequestParam>>,
    filter: Arc<Option<Filter>>,
}

impl McpClient {
    /// Blacklist tools by name, the agent will not be able to use these tools
    pub fn with_blacklist<ITEM: Into<String>, I: IntoIterator<Item = ITEM>>(
        &mut self,
        blacklist: I,
    ) -> &mut Self {
        let list = blacklist.into_iter().map(Into::into).collect::<Vec<_>>();
        self.filter = Some(Filter::Blacklist(list)).into();
        self
    }

    /// Whitelist tools by name, the agent will only be able to use these tools
    pub fn with_whitelist<ITEM: Into<String>, I: IntoIterator<Item = ITEM>>(
        &mut self,
        blacklist: I,
    ) -> &mut Self {
        let list = blacklist.into_iter().map(Into::into).collect::<Vec<_>>();
        self.filter = Some(Filter::Whitelist(list)).into();
        self
    }

    /// Create a new client from a transport
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
        let client = Arc::new(info.serve(transport).await?);

        Ok(Self {
            client,
            filter: None.into(),
        })
    }

    /// Create a new client from a running service
    pub fn from_running_service(
        client: impl Into<Arc<RunningService<RoleClient, InitializeRequestParam>>>,
    ) -> Self {
        Self {
            client: client.into(),
            filter: None.into(),
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
}

#[derive(Deserialize)]
struct ToolInputSchema {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub type_: String, // This _must_ be object
    pub properties: Option<HashMap<String, Value>>,
    pub required: Option<Vec<String>>,
}

#[async_trait]
impl ToolBox for McpClient {
    #[tracing::instrument(skip_all)]
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        let tools = self
            .client
            .list_tools(None)
            .await
            .context("Failed to list tools")?;

        let tools = tools
            .tools
            .into_iter()
            .map(|t| {
                let schema: ToolInputSchema = serde_json::from_value(t.schema_as_json_value())
                    .context("Failed to parse tool input schema")?;

                let mut tool_spec = ToolSpec::builder()
                    .name(t.name.clone())
                    .description(t.description)
                    .to_owned();
                let mut parameters = Vec::new();

                if let Some(p) = schema.properties {
                    for (name, value) in &p {
                        let param = ParamSpec::builder()
                            .name(name)
                            .description(
                                value
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .unwrap_or(""),
                            )
                            .ty(value.get("type").map_or(ParamType::String, |t| {
                                serde_json::from_value(t.clone()).unwrap_or(ParamType::String)
                            }))
                            .build()
                            .context("Failed to build parameters")
                            .unwrap();

                        parameters.push(param);
                    }
                }

                tool_spec.parameters(parameters);
                let tool_spec = tool_spec.build().context("Failed to build tool spec")?;

                Ok(Box::new(McpTool {
                    client: self.client.clone(),
                    tool_name: t.name.into(),
                    tool_spec,
                }) as Box<dyn Tool>)
            })
            .collect::<Result<Vec<_>>>()?;

        if let Some(filter) = self.filter.as_ref() {
            match filter {
                Filter::Blacklist(blacklist) => {
                    let blacklist = blacklist.iter().map(String::as_str).collect::<Vec<_>>();
                    Ok(tools
                        .into_iter()
                        .filter(|t| !blacklist.contains(&t.name().as_ref()))
                        .collect())
                }
                Filter::Whitelist(whitelist) => {
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
}

#[derive(Clone)]
struct McpTool {
    client: Arc<RunningService<RoleClient, InitializeRequestParam>>,
    tool_name: String,
    tool_spec: ToolSpec,
}

#[async_trait]
impl Tool for McpTool {
    async fn invoke(
        &self,
        _agent_context: &dyn swiftide_core::AgentContext,
        raw_args: Option<&str>,
    ) -> Result<
        swiftide_core::chat_completion::ToolOutput,
        swiftide_core::chat_completion::errors::ToolError,
    > {
        let args = match raw_args {
            Some(args) => Some(serde_json::from_str(args)?),
            None => None,
        };

        let request = CallToolRequestParam {
            name: self.tool_name.clone().into(),
            arguments: args,
        };

        tracing::debug!(request = ?request, tool = self.name().as_ref(), "Invoking mcp tool");
        let response = self
            .client
            .call_tool(request)
            .await
            .context("Failed to call mcp tool")?;

        tracing::debug!(response = ?response, tool = self.name().as_ref(), "Received response from mcp tool");
        let content = response
            .content
            .into_iter()
            .filter_map(|c| c.as_text().map(|t| t.text.to_string()))
            .collect::<Vec<_>>()
            .join("\n");

        if let Some(error) = response.is_error {
            if error {
                return Err(ToolError::Unknown(anyhow::anyhow!(
                    "Failed to execute mcp tool: {content}"
                )));
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

#[cfg(test)]
mod tests {
    use super::*;
    use copied_from_rmcp::Calculator;
    use rmcp::serve_server;
    use tokio::net::{UnixListener, UnixStream};

    const SOCKET_PATH: &str = "/tmp/swiftide-mcp.sock";

    #[allow(clippy::similar_names)]
    #[test_log::test(tokio::test(flavor = "multi_thread"))]
    async fn test_socket() -> Result<()> {
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

        let client = client().await?;

        let t = client.available_tools().await?;
        assert_eq!(client.available_tools().await?.len(), 2);

        let mut names = t.iter().map(|t| t.name()).collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["sub", "sum"]);

        let sum_tool = t.iter().find(|t| t.name() == "sum").unwrap();
        assert_eq!(
            sum_tool.tool_spec(),
            ToolSpec::builder().name("sub").build()?
        );
        let result = sum_tool
            .invoke(&(), Some(r#"{"a": 10, "b": 20}"#))
            .await?
            .content()
            .unwrap()
            .to_string();
        assert_eq!(result, "30");

        let sub_tool = t.iter().find(|t| t.name() == "sub").unwrap();
        assert_eq!(
            sub_tool.tool_spec(),
            ToolSpec::builder().name("sub").build()?
        );
        let result = sub_tool
            .invoke(&(), Some(r#"{"a": 10, "b": 20}"#))
            .await?
            .content()
            .unwrap()
            .to_string();
        assert_eq!(result, "-10");

        // Clean up socket file
        let _ = std::fs::remove_file(SOCKET_PATH);

        Ok(())
    }

    async fn server(unix_listener: UnixListener) -> anyhow::Result<()> {
        while let Ok((stream, addr)) = unix_listener.accept().await {
            println!("Client connected: {addr:?}");
            tokio::spawn(async move {
                match serve_server(Calculator, stream).await {
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

    async fn client() -> anyhow::Result<McpClient> {
        println!("Client connecting to {SOCKET_PATH}");
        let stream = UnixStream::connect(SOCKET_PATH).await?;

        // let client = serve_client((), stream).await?;
        let client = McpClient::try_from_transport(stream).await?;
        println!("Client connected and initialized successfully");

        Ok(client)
    }

    #[allow(clippy::unused_self)]
    mod copied_from_rmcp {
        use rmcp::{
            model::{ServerCapabilities, ServerInfo},
            schemars, tool, ServerHandler,
        };

        #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
        pub struct SumRequest {
            #[schemars(description = "the left hand side number")]
            pub a: i32,
            pub b: i32,
        }
        #[derive(Debug, Clone)]
        pub struct Calculator;
        #[rmcp::tool(tool_box)]
        impl Calculator {
            #[tool(description = "Calculate the sum of two numbers")]
            fn sum(&self, #[tool(aggr)] SumRequest { a, b }: SumRequest) -> String {
                (a + b).to_string()
            }

            #[tool(description = "Calculate the sum of two numbers")]
            fn sub(
                &self,
                #[tool(param)]
                #[schemars(description = "the left hand side number", required)]
                a: i32,
                #[tool(param)]
                #[schemars(description = "the left hand side number")]
                b: i32,
            ) -> String {
                (a - b).to_string()
            }
        }

        #[rmcp::tool(tool_box)]
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
