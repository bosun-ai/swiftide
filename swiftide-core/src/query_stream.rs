use anyhow::Result;
use std::pin::Pin;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use futures_util::stream::Stream;
pub use futures_util::{StreamExt, TryStreamExt};
use pin_project_lite::pin_project;

use crate::querying::Query;

pin_project! {
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

impl<'stream, Q> Into<QueryStream<'stream, Q>>
    for Pin<Box<dyn Stream<Item = Result<Query<Q>>> + Send>>
{
    fn into(self) -> QueryStream<'stream, Q> {
        QueryStream {
            inner: self,
            sender: None,
        }
    }
}
