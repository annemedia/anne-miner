
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use core::future::Future; 
use futures_core::stream::Stream;
use tokio::time::{sleep_until, Sleep};

/// A stream representing notifications at a fixed interval *after* processing completes.
/// Unlike Tokio's default interval, this implementation starts the countdown after each item completes.
#[derive(Debug)]
pub struct Interval {
    sleep: Pin<Box<Sleep>>,
    duration: Duration,
}

impl Interval {
    /// Create a new `Interval` that starts at `at` and yields every `duration` interval after processing.
    pub fn new(at: Instant, duration: Duration) -> Self {
        assert!(
            duration > Duration::ZERO,
            "`duration` must be non-zero."
        );

        let sleep = Box::pin(sleep_until(at.into()));
        Self { sleep, duration }
    }

    pub fn new_interval(duration: Duration) -> Self {
        Self::new(Instant::now() + duration, duration)
    }
}

impl Stream for Interval {
    type Item = Instant;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.sleep.as_mut().poll(cx).is_pending() {
            return Poll::Pending;
        }

        let now = Instant::now();
        let duration = self.duration;
        self.sleep.as_mut().reset((now + duration).into());

        Poll::Ready(Some(now))
    }

}