// show feature flags in the generated documentation
// https://doc.rust-lang.org/rustdoc/unstable-features.html#extensions-to-the-doc-attribute
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc(html_logo_url = "https://github.com/bosun-ai/swiftide/raw/master/images/logo.png")]

//! Swiftide agents are a flexible way to build fast and reliable AI agents.
//!
//! # Features
//!
//! * **Tools**: Tools can be defined as functions using the `#[tool]` attribute macro, the `Tool`
//!   derive macro, or manually implementing the `Tool` trait.
//! * **Hooks**: At various stages of the agent lifecycle, hooks can be defined to run custom logic.
//!   These are defined when building the agent, and each take a closure.
//! * **Context**: Agents operate in an `AgentContext`, which is a shared state between tools and
//!   hooks. The context is responsible for managing the completions and interacting with the
//!   outside world.
//! * **Tool Execution**: A context takes a tool executor (local by default) to execute its tools
//!   on. This enables tools to be run i.e. in containers, remote, etc.
//! * **System prompt defaults**: `SystemPrompt` provides a default, customizable prompt for the
//!   agent. If you want to provider your own prompt, the builder takes anything that converts into
//!   a `Prompt`, including strings.
//! * **Open Telemetry**: Agents are fully instrumented with open telemetry.
//!
//! # Example
//!
//! ```ignore
//! # use swiftide_agents::Agent;
//! # use swiftide_integrations as integrations;
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let openai = integrations::openai::OpenAI::builder()
//!     .default_prompt_model("gpt-4o-mini")
//!     .build()?;
//!
//! Agent::builder()
//!     .llm(&openai)
//!     .before_completion(move |_,_|
//!         Box::pin(async move {
//!                 println!("Before each tool");
//!                 Ok(())
//!             })
//!     )
//!     .build()?
//!     .query("What is the meaning of life?")
//!     .await?;
//! # return Ok(());
//!
//! # }
//! ```
//!
//! Agents run in a loop as long as they have new messages to process.
mod agent;
mod default_context;
pub mod errors;
pub mod hooks;
mod state;
pub mod system_prompt;
pub mod tools;
mod util;

pub use agent::Agent;
pub use default_context::DefaultContext;

#[cfg(test)]
mod test_utils;
