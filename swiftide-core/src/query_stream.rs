//! Internally used by a query pipeline
//!
//! Has a sender and receiver to initialize the stream
use anyhow::Result;
use std::pin::Pin;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use futures_util::stream::Stream;
pub use futures_util::{StreamExt, TryStreamExt};

use crate::{query::QueryState, querying::Query};

/// Internally used by a query pipeline
///
/// Has a sender and receiver to initialize the stream
#[pin_project::pin_project]
pub struct QueryStream<'stream, STATE: 'stream + QueryState> {
    #[pin]
    pub(crate) inner: Pin<Box<dyn Stream<Item = Result<Query<STATE>>> + Send + 'stream>>,

    #[pin]
    pub sender: Option<Sender<Result<Query<STATE>>>>,
}

impl<'stream, STATE: QueryState + 'stream> Default for QueryStream<'stream, STATE> {
    fn default() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        Self {
            inner: ReceiverStream::new(receiver).boxed(),
            sender: Some(sender),
        }
    }
}

impl<STATE: QueryState> Stream for QueryStream<'_, STATE> {
    type Item = Result<Query<STATE>>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        this.inner.poll_next(cx)
    }
}

impl<STATE: QueryState> From<Pin<Box<dyn Stream<Item = Result<Query<STATE>>> + Send>>>
    for QueryStream<'_, STATE>
{
    fn from(val: Pin<Box<dyn Stream<Item = Result<Query<STATE>>> + Send>>) -> Self {
        QueryStream {
            inner: val,
            sender: None,
        }
    }
}
