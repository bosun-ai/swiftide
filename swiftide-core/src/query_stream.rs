//! Internally used by a query pipeline
//!
//! Has a sender and receiver to initialize the stream
use anyhow::Result;
use std::pin::Pin;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use futures_util::stream::Stream;
pub use futures_util::{StreamExt, TryStreamExt};
use pin_project_lite::pin_project;

use crate::querying::Query;

pin_project! {
    /// Internally used by a query pipeline
    ///
    /// Has a sender and receiver to initialize the stream
    pub struct QueryStream<'stream, Q: 'stream> {
        #[pin]
        pub(crate) inner: Pin<Box<dyn Stream<Item = Result<Query<Q>>> + Send + 'stream>>,

        #[pin]
        pub sender: Option<Sender<Result<Query<Q>>>>
    }
}

impl<'stream, T: Send + Sync + 'stream> Default for QueryStream<'stream, T> {
    fn default() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        Self {
            inner: ReceiverStream::new(receiver).boxed(),
            sender: Some(sender),
        }
    }
}

impl<'stream, Q: Send> Stream for QueryStream<'stream, Q> {
    type Item = Result<Query<Q>>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        this.inner.poll_next(cx)
    }
}

impl<'stream, Q> From<Pin<Box<dyn Stream<Item = Result<Query<Q>>> + Send>>>
    for QueryStream<'stream, Q>
{
    fn from(val: Pin<Box<dyn Stream<Item = Result<Query<Q>>> + Send>>) -> Self {
        QueryStream {
            inner: val,
            sender: None,
        }
    }
}
