#![allow(clippy::from_over_into)]
//! This module defines the `IngestionStream` type, which is used for handling asynchronous streams of `IngestionNode` items in the ingestion pipeline.

use anyhow::Result;
use futures_util::stream::{self, Stream};
use pin_project_lite::pin_project;
use std::pin::Pin;
use tokio::sync::mpsc::Receiver;

use super::IngestionNode;

pub use futures_util::{StreamExt, TryStreamExt};

// We need to inform the compiler that `inner` is pinned as well
pin_project! {
    /// An asynchronous stream of `IngestionNode` items.
    ///
    /// Wraps an internal stream of `Result<IngestionNode>` items.
    ///
    /// Streams, iterators and vectors of `Result<IngestionNode>` can be converted into an `IngestionStream`.
    pub struct IngestionStream {
        #[pin]
        pub(crate) inner: Pin<Box<dyn Stream<Item = Result<IngestionNode>> + Send>>,
    }
}

impl Stream for IngestionStream {
    type Item = Result<IngestionNode>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        this.inner.poll_next(cx)
    }
}

impl Into<IngestionStream> for Vec<Result<IngestionNode>> {
    fn into(self) -> IngestionStream {
        IngestionStream::iter(self)
    }
}

impl Into<IngestionStream> for Result<Vec<IngestionNode>> {
    fn into(self) -> IngestionStream {
        match self {
            Ok(nodes) => IngestionStream::iter(nodes.into_iter().map(Ok)),
            Err(err) => IngestionStream::iter(vec![Err(err)]),
        }
    }
}

impl Into<IngestionStream> for Pin<Box<dyn Stream<Item = Result<IngestionNode>> + Send>> {
    fn into(self) -> IngestionStream {
        IngestionStream { inner: self }
    }
}

impl Into<IngestionStream> for Receiver<Result<IngestionNode>> {
    fn into(self) -> IngestionStream {
        IngestionStream {
            inner: tokio_stream::wrappers::ReceiverStream::new(self).boxed(),
        }
    }
}

impl IngestionStream {
    pub fn empty() -> Self {
        IngestionStream {
            inner: stream::empty().boxed(),
        }
    }

    // NOTE: Can we really guarantee that the iterator will outlive the stream?
    pub fn iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Result<IngestionNode>> + Send + 'static,
        <I as IntoIterator>::IntoIter: Send,
    {
        IngestionStream {
            inner: stream::iter(iter).boxed(),
        }
    }
}
