use anyhow::Result;
use std::pin::Pin;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use futures_util::stream::Stream;
pub use futures_util::{StreamExt, TryStreamExt};
use pin_project_lite::pin_project;

use super::Query;

pin_project! {
    pub struct QueryStream {
        #[pin]
        pub(crate) inner: Pin<Box<dyn Stream<Item = Result<Query>> + Send>>,

        #[pin]
        pub(crate) sender: Option<Sender<Result<Query>>>
    }
}

impl Default for QueryStream {
    fn default() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        Self {
            inner: ReceiverStream::new(receiver).boxed(),
            sender: Some(sender),
        }
    }
}

impl Stream for QueryStream {
    type Item = Result<Query>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        this.inner.poll_next(cx)
    }
}

impl Into<QueryStream> for Pin<Box<dyn Stream<Item = Result<Query>> + Send>> {
    fn into(self) -> QueryStream {
        QueryStream {
            inner: self,
            sender: None,
        }
    }
}
