#![allow(clippy::from_over_into)]
#![cfg(not(tarpaulin_include))]
//! This module defines the `IngestionStream` type, which is used for handling asynchronous streams of `IngestionNode` items in the indexing pipeline.

use anyhow::Result;
use futures_util::stream::{self, Stream};
use pin_project_lite::pin_project;
use std::pin::Pin;
use tokio::sync::mpsc::Receiver;

use super::Node;

pub use futures_util::{StreamExt, TryStreamExt};

// We need to inform the compiler that `inner` is pinned as well
pin_project! {
    /// An asynchronous stream of `IngestionNode` items.
    ///
    /// Wraps an internal stream of `Result<IngestionNode>` items.
    ///
    /// Streams, iterators and vectors of `Result<IngestionNode>` can be converted into an `IngestionStream`.
    pub struct IndexingStream {
        #[pin]
        pub(crate) inner: Pin<Box<dyn Stream<Item = Result<Node>> + Send>>,
    }
}

impl Stream for IndexingStream {
    type Item = Result<Node>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        this.inner.poll_next(cx)
    }
}

impl Into<IndexingStream> for Vec<Result<Node>> {
    fn into(self) -> IndexingStream {
        IndexingStream::iter(self)
    }
}

impl Into<IndexingStream> for Result<Vec<Node>> {
    fn into(self) -> IndexingStream {
        match self {
            Ok(nodes) => IndexingStream::iter(nodes.into_iter().map(Ok)),
            Err(err) => IndexingStream::iter(vec![Err(err)]),
        }
    }
}

impl Into<IndexingStream> for Pin<Box<dyn Stream<Item = Result<Node>> + Send>> {
    fn into(self) -> IndexingStream {
        IndexingStream { inner: self }
    }
}

impl Into<IndexingStream> for Receiver<Result<Node>> {
    fn into(self) -> IndexingStream {
        IndexingStream {
            inner: tokio_stream::wrappers::ReceiverStream::new(self).boxed(),
        }
    }
}

impl IndexingStream {
    pub fn empty() -> Self {
        IndexingStream {
            inner: stream::empty().boxed(),
        }
    }

    // NOTE: Can we really guarantee that the iterator will outlive the stream?
    pub fn iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Result<Node>> + Send + 'static,
        <I as IntoIterator>::IntoIter: Send,
    {
        IndexingStream {
            inner: stream::iter(iter).boxed(),
        }
    }
}
