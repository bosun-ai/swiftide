#![allow(clippy::from_over_into)]

//! This module defines the `IndexingStream` type, which is used internally by a pipeline  for
//! handling asynchronous streams of `Node<T>` items in the indexing pipeline.

use crate::node::{Chunk, Node};
use anyhow::Result;
use futures_util::stream::{self, Stream};
use std::pin::Pin;
use tokio::sync::mpsc::Receiver;

pub use futures_util::StreamExt;

// We need to inform the compiler that `inner` is pinned as well
/// An asynchronous stream of `Node<T>` items.
///
/// Wraps an internal stream of `Result<Node<T>>` items.
///
/// Streams, iterators and vectors of `Result<Node<T>>` can be converted into an `IndexingStream`.
#[pin_project::pin_project]
pub struct IndexingStream<T: Chunk> {
    #[pin]
    pub(crate) inner: Pin<Box<dyn Stream<Item = Result<Node<T>>> + Send>>,
}

impl<T: Chunk> Stream for IndexingStream<T> {
    type Item = Result<Node<T>>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        this.inner.poll_next(cx)
    }
}

impl<T: Chunk> Into<IndexingStream<T>> for Vec<Result<Node<T>>> {
    fn into(self) -> IndexingStream<T> {
        IndexingStream::iter(self)
    }
}

impl<T: Chunk> Into<IndexingStream<T>> for Vec<Node<T>> {
    fn into(self) -> IndexingStream<T> {
        IndexingStream::from_nodes(self)
    }
}

// impl Into<IndexingStream> for anyhow::Error {
//     fn into(self) -> IndexingStream {
//         IndexingStream::iter(vec![Err(self)])
//     }
// }

impl<T: Chunk> Into<IndexingStream<T>> for Result<Vec<Node<T>>> {
    fn into(self) -> IndexingStream<T> {
        match self {
            Ok(nodes) => IndexingStream::iter(nodes.into_iter().map(Ok)),
            Err(err) => IndexingStream::iter(vec![Err(err)]),
        }
    }
}

impl<T: Chunk> Into<IndexingStream<T>> for Pin<Box<dyn Stream<Item = Result<Node<T>>> + Send>> {
    fn into(self) -> IndexingStream<T> {
        IndexingStream { inner: self }
    }
}

impl<T: Chunk> Into<IndexingStream<T>> for Receiver<Result<Node<T>>> {
    fn into(self) -> IndexingStream<T> {
        IndexingStream {
            inner: tokio_stream::wrappers::ReceiverStream::new(self).boxed(),
        }
    }
}

impl<T: Chunk> From<anyhow::Error> for IndexingStream<T> {
    fn from(err: anyhow::Error) -> Self {
        IndexingStream::iter(vec![Err(err)])
    }
}

impl<T: Chunk> IndexingStream<T> {
    pub fn empty() -> Self {
        IndexingStream {
            inner: stream::empty().boxed(),
        }
    }

    /// Creates an `IndexingStream` from an iterator of `Result<Node<T>>`.
    ///
    /// WARN: Also works with Err items directly, which will result
    /// in an _incorrect_ stream
    pub fn iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Result<Node<T>>> + Send + 'static,
        <I as IntoIterator>::IntoIter: Send,
    {
        IndexingStream {
            inner: stream::iter(iter).boxed(),
        }
    }

    pub fn from_nodes(nodes: Vec<Node<T>>) -> Self {
        IndexingStream::iter(nodes.into_iter().map(Ok))
    }
}
