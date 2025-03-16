// show feature flags in the generated documentation
// https://doc.rust-lang.org/rustdoc/unstable-features.html#extensions-to-the-doc-attribute
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc(html_logo_url = "https://github.com/bosun-ai/swiftide/raw/master/images/logo.png")]

pub mod answers;
mod query;
pub mod query_transformers;
pub mod response_transformers;

pub use query::*;
pub mod evaluators;
