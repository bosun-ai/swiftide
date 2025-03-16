// show feature flags in the generated documentation
// https://doc.rust-lang.org/rustdoc/unstable-features.html#extensions-to-the-doc-attribute
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc(html_logo_url = "https://github.com/bosun-ai/swiftide/raw/master/images/logo.png")]

#[cfg(feature = "test-utils")]
mod test_utils;

#[cfg(feature = "test-utils")]
pub use test_utils::*;
