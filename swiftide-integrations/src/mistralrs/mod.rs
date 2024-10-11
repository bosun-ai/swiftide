//! Use mistral.rs right inside your pipeline
//!
//! Mistral.rs supports a variety of hugging face models.
//!
//! See [the mistral.rs github page](https://github.com/EricLBuehler/mistral.rs/) for more
//! information.
mod simple_prompt;

pub use simple_prompt::{MistralTextModel, MistralTextModelBuilder};
