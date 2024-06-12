use anyhow::Result;
use futures_util::stream::Stream;
use std::pin::Pin;

use super::IngestionNode;

pub type IngestionStream = Pin<Box<dyn Stream<Item = Result<IngestionNode>> + Send>>;
