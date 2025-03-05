use std::{sync::Arc, time::Duration};

use async_mcp::{protocol::RequestOptions, transport::Transport};
use async_trait::async_trait;
use serde_json::json;
use swiftide_core::{chat_completion::ToolSpec, Tool};

struct McpClient<T: Transport> {
    client: Arc<async_mcp::client::Client<T>>,
}

impl<T: Transport> McpClient<T> {
    // {
    //   "jsonrpc": "2.0",
    //   "id": 1,
    //   "result": {
    //     "tools": [
    //       {
    //         "name": "get_weather",
    //         "description": "Get current weather information for a location",
    //         "inputSchema": {
    //           "type": "object",
    //           "properties": {
    //             "location": {
    //               "type": "string",
    //               "description": "City name or zip code"
    //             }
    //           },
    //           "required": ["location"]
    //         }
    //       }
    //     ],
    //     "nextCursor": "next-page-cursor"
    //   }
    // }
    pub async fn list_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        let specs =

    }

}

#[derive(Clone)]
struct McpTool<T: Transport> {
    client: Arc<async_mcp::client::Client<T>>,
    tool_spec: ToolSpec,
}

#[async_trait]
impl<T: Transport + Clone> Tool for McpTool<T> {
    async fn invoke(
        &self,
        _agent_context: &dyn swiftide_core::AgentContext,
        raw_args: Option<&str>,
    ) -> Result<
        swiftide_core::chat_completion::ToolOutput,
        swiftide_core::chat_completion::errors::ToolError,
    > {
        // TODO:  validate the input based on the tool spec

        let args = serde_json::from_str(raw_args.unwrap_or("{}"))?;
        let response = self
            .client
            .request(
                "tools/call",
                Some(json!({
                    "name": self.name(),
                    "arguments": args
                })),
                RequestOptions::default().timeout(Duration::from_secs(5)),
            )
            .await?; // TODO: Handle this error properly
                     //
                     //

        // TODO: Do something with this value, should parse like `{ content: { type: text, text: string} }`, tool errors should have a handleable 'isError' in the root
        todo!()
    }

    fn name(&self) -> std::borrow::Cow<'_, str> {
        self.tool_spec().name.into()
    }

    fn tool_spec(&self) -> ToolSpec {
        self.tool_spec.clone()
    }
}
