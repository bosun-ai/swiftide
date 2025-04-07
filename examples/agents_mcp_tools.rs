//! This is an example of how to build a Swiftide agent with tools using the MCP protocol.
//!
//! The agent in this example prints all messages using a channel.
use anyhow::Result;
use rmcp::{
    model::{ClientInfo, Implementation},
    transport::TokioChildProcess,
    ServiceExt as _,
};
use swiftide::agents::{self, tools::mcp::McpToolbox};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Hello, agents!");

    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_embed_model("text-embeddings-3-small")
        .default_prompt_model("gpt-4o-mini")
        .build()?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("{}", msg);
        }
    });

    // First set up our client info to identify ourselves to the server
    let client_info = ClientInfo {
        client_info: Implementation {
            name: "swiftide-example".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        ..Default::default()
    };

    // Use `rmcp` to start the server
    let running_service = client_info
        .serve(TokioChildProcess::new(
            tokio::process::Command::new("npx")
                .args(["-y", "@modelcontextprotocol/server-everything"]),
        )?)
        .await?;

    // Create a toolbox from the running server, and only use the `add` tool
    //
    // A toolbox reveals it's tools to the swiftide agent the first time it starts (if the state of
    // the agent was pending). You can add as many toolboxes as you want. MCP services are an
    // implmenentation of a toolbox. A list of tools is another.
    let everything_toolbox = McpToolbox::from_running_service(running_service)
        .with_whitelist(["add"])
        .to_owned();

    agents::Agent::builder()
        .llm(&openai)
        // Add the toolbox to the agent
        .add_toolbox(everything_toolbox)
        // Every message added by the agent will be printed to stdout
        .on_new_message(move |_, msg| {
            let msg = msg.to_string();
            let tx = tx.clone();
            Box::pin(async move {
                tx.send(msg).unwrap();
                Ok(())
            })
        })
        .build()?
        .query("Use the add tool to add 1 and 2")
        .await?;

    Ok(())
}
