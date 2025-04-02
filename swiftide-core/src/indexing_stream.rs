#![allow(clippy::from_over_into)]

//! This module defines the `IndexingStream` type, which is used internally by a pipeline  for
//! handling asynchronous streams of `Node` items in the indexing pipeline.

use crate::node::Node;
use anyhow::Result;
use futures_util::stream::{self, Stream};
use std::pin::Pin;
use tokio::sync::mpsc::Receiver;

pub use futures_util::StreamExt;

// We need to inform the compiler that `inner` is pinned as well
/// An asynchronous stream of `Node` items.
///
/// Wraps an internal stream of `Result<Node>` items.
///
/// Streams, iterators and vectors of `Result<Node>` can be converted into an `IndexingStream`.
#[pin_project::pin_project]
pub struct IndexingStream {
    #[pin]
    pub(crate) inner: Pin<Box<dyn Stream<Item = Result<Node>> + Send>>,
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

impl Into<IndexingStream> for Vec<Node> {
    fn into(self) -> IndexingStream {
        IndexingStream::from_nodes(self)
    }
}

// impl Into<IndexingStream> for anyhow::Error {
//     fn into(self) -> IndexingStream {
//         IndexingStream::iter(vec![Err(self)])
//     }
// }

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

impl From<anyhow::Error> for IndexingStream {
    fn from(err: anyhow::Error) -> Self {
        IndexingStream::iter(vec![Err(err)])
    }
}

impl IndexingStream {
    pub fn empty() -> Self {
        IndexingStream {
            inner: stream::empty().boxed(),
        }
    }

    /// Creates an `IndexingStream` from an iterator of `Result<Node>`.
    ///
    /// WARN: Also works with Err items directly, which will result
    /// in an _incorrect_ stream
    pub fn iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Result<Node>> + Send + 'static,
        <I as IntoIterator>::IntoIter: Send,
    {
        IndexingStream {
            inner: stream::iter(iter).boxed(),
        }
    }

    pub fn from_nodes(nodes: Vec<Node>) -> Self {
        IndexingStream::iter(nodes.into_iter().map(Ok))
    }
}
