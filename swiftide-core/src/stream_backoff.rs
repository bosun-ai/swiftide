// Credits go to https://github.com/ihrwein/backoff/pull/50
use std::{pin::Pin, task::Poll, time::Duration};

use backoff::{backoff::Backoff, future::Sleeper};
use futures_util::{Stream, TryStream};
use pin_project::pin_project;

// /// Applies a [`Backoff`] policy to a [`Stream`]
// ///
// /// After any [`Err`] is emitted, the stream is paused for [`Backoff::next_backoff`]. The
// /// [`Backoff`] is [`reset`](`Backoff::reset`) on any [`Ok`] value.
// ///
// /// If [`Backoff::next_backoff`] returns [`None`] then the backing stream is given up on, and
// closed. pub fn backoff<S: TryStream, B: Backoff>(
//     stream: S,
//     backoff: B,
// ) -> StreamBackoff<S, B, impl Sleeper> {
//     StreamBackoff::new(stream, backoff, TokioSleeper)
// }

pub(crate) struct TokioSleeper;
impl Sleeper for TokioSleeper {
    type Sleep = ::tokio::time::Sleep;
    fn sleep(&self, dur: Duration) -> Self::Sleep {
        ::tokio::time::sleep(dur)
    }
}

/// See [`backoff`]
#[pin_project]
pub struct StreamBackoff<S, B, Sl: Sleeper> {
    #[pin]
    stream: S,
    backoff: B,
    sleeper: Sl,
    #[pin]
    state: State<Sl>,
}

#[pin_project(project = StateProj)]
enum State<Sl: Sleeper> {
    BackingOff {
        #[pin]
        backoff_sleep: Sl::Sleep,
    },
    GivenUp,
    Awake,
}

impl<S: TryStream, B: Backoff, Sl: Sleeper> StreamBackoff<S, B, Sl> {
    pub fn new(stream: S, backoff: B, sleeper: Sl) -> Self {
        Self {
            stream,
            backoff,
            sleeper,
            state: State::Awake,
        }
    }
}

impl<S: TryStream, B: Backoff, Sl: Sleeper> Stream for StreamBackoff<S, B, Sl>
where
    Sl::Sleep: Future,
{
    type Item = Result<S::Ok, S::Error>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        match this.state.as_mut().project() {
            StateProj::BackingOff { mut backoff_sleep } => match backoff_sleep.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    // tracing::debug!(deadline = ?backoff_sleep.deadline(), "Backoff complete,
                    // waking up");
                    this.state.set(State::Awake);
                }
                Poll::Pending => {
                    // let deadline = backoff_sleep.deadline();
                    // tracing::trace!(
                    //     ?deadline,
                    //     remaining_duration = ?deadline.saturating_duration_since(Instant::now()),
                    //     "Still waiting for backoff sleep to complete"
                    // );
                    return Poll::Pending;
                }
            },
            StateProj::GivenUp => {
                // tracing::debug!("Backoff has given up, stream is closed");
                return Poll::Ready(None);
            }
            StateProj::Awake => {}
        }

        let next_item = this.stream.try_poll_next(cx);
        match &next_item {
            Poll::Ready(Some(Err(_))) => {
                if let Some(backoff_duration) = this.backoff.next_backoff() {
                    let backoff_sleep = this.sleeper.sleep(backoff_duration);
                    // tracing::debug!(
                    //     deadline = ?backoff_sleep.deadline(),
                    //     duration = ?backoff_duration,
                    //     "Error received, backing off"
                    // );
                    this.state.set(State::BackingOff { backoff_sleep });
                } else {
                    // tracing::debug!("Error received, giving up");
                    this.state.set(State::GivenUp);
                }
            }
            Poll::Ready(_) => {
                // tracing::trace!("Non-error received, resetting backoff");
                this.backoff.reset();
            }
            Poll::Pending => {}
        }
        next_item
    }
}

// Tokio clock is required to be able to freeze time during marble tests
#[cfg(test)]
mod tests {
    use super::*;

    use futures_util::{StreamExt, pin_mut, poll, stream};
    use std::{task::Poll, time::Duration};
    use tokio::{self, sync::mpsc};

    #[tokio::test]
    async fn stream_should_back_off() {
        tokio::time::pause();
        let tick = Duration::from_secs(1);
        let rx = stream::iter([Ok(0), Ok(1), Err(2), Ok(3), Ok(4)]);
        let rx = StreamBackoff::new(rx, backoff::backoff::Constant::new(tick), TokioSleeper);
        pin_mut!(rx);
        assert_eq!(poll!(rx.next()), Poll::Ready(Some(Ok(0))));
        assert_eq!(poll!(rx.next()), Poll::Ready(Some(Ok(1))));
        assert_eq!(poll!(rx.next()), Poll::Ready(Some(Err(2))));
        assert_eq!(poll!(rx.next()), Poll::Pending);
        tokio::time::advance(tick * 2).await;
        assert_eq!(poll!(rx.next()), Poll::Ready(Some(Ok(3))));
        assert_eq!(poll!(rx.next()), Poll::Ready(Some(Ok(4))));
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn backoff_time_should_update() {
        tokio::time::pause();
        let (tx, rx) = mpsc::unbounded_channel();
        let rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        let rx = StreamBackoff::new(rx, LinearBackoff::new(Duration::from_secs(2)), TokioSleeper);
        pin_mut!(rx);
        tx.send(Ok(0)).unwrap();
        assert_eq!(poll!(rx.next()), Poll::Ready(Some(Ok(0))));
        tx.send(Ok(1)).unwrap();
        assert_eq!(poll!(rx.next()), Poll::Ready(Some(Ok(1))));
        tx.send(Err(2)).unwrap();
        assert_eq!(poll!(rx.next()), Poll::Ready(Some(Err(2))));
        assert_eq!(poll!(rx.next()), Poll::Pending);
        tokio::time::advance(Duration::from_secs(3)).await;
        assert_eq!(poll!(rx.next()), Poll::Pending);
        tx.send(Err(3)).unwrap();
        assert_eq!(poll!(rx.next()), Poll::Ready(Some(Err(3))));
        tx.send(Ok(4)).unwrap();
        assert_eq!(poll!(rx.next()), Poll::Pending);
        tokio::time::advance(Duration::from_secs(3)).await;
        assert_eq!(poll!(rx.next()), Poll::Pending);
        tokio::time::advance(Duration::from_secs(2)).await;
        assert_eq!(poll!(rx.next()), Poll::Ready(Some(Ok(4))));
        assert_eq!(poll!(rx.next()), Poll::Pending);
        drop(tx);
        assert_eq!(poll!(rx.next()), Poll::Ready(None));
    }

    #[tokio::test]
    async fn backoff_should_close_when_requested() {
        assert_eq!(
            StreamBackoff::new(
                stream::iter([Ok(0), Ok(1), Err(2), Ok(3)]),
                backoff::backoff::Stop {},
                TokioSleeper
            )
            .collect::<Vec<_>>()
            .await,
            vec![Ok(0), Ok(1), Err(2)]
        );
    }

    /// Dynamic backoff policy that is still deterministic and testable
    struct LinearBackoff {
        interval: Duration,
        current_duration: Duration,
    }

    impl LinearBackoff {
        fn new(interval: Duration) -> Self {
            Self {
                interval,
                current_duration: Duration::ZERO,
            }
        }
    }

    impl Backoff for LinearBackoff {
        fn next_backoff(&mut self) -> Option<Duration> {
            self.current_duration += self.interval;
            Some(self.current_duration)
        }

        fn reset(&mut self) {
            self.current_duration = Duration::ZERO;
        }
    }
}
