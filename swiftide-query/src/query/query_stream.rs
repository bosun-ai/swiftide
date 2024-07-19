pin_project! {
pub struct IndexingStream {
    #[pin]
    pub(crate) inner: Pin<Box<dyn Stream<Item = Result<Query>> + Send>>,
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
