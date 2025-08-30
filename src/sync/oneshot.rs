//! A wrapper around [`tokio::sync::oneshot::Receiver`] that implements [`Stream`].
//!
//! ! [`tokio::sync::oneshot::Receiver`]: struct@tokio::sync::oneshot::Receiver
//! ! [`Stream`]: trait@tokio_stream::Stream
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot::Receiver;
use tokio_stream::Stream;

/// A wrapper around [`tokio::sync::oneshot::Receiver`] that implements [`Stream`].
///
/// [`tokio::sync::oneshot::Receiver`]: struct@tokio::sync::oneshot::Receiver
/// [`Stream`]: trait@crate::Stream
#[derive(Debug)]
#[repr(transparent)]
pub struct ReceiverStream<T>(pub Receiver<T>);

impl<T> ReceiverStream<T> {
    /// Create a new `ReceiverStream`.
    #[inline(always)]
    pub fn new(recv: Receiver<T>) -> Self {
        Self(recv)
    }

    /// Get back the inner `Receiver`.
    #[inline(always)]
    pub fn into_inner(self) -> Receiver<T> {
        self.0
    }

    /// Closes the receiving half of a channel without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered. Any
    /// outstanding [`Permit`] values will still be able to send messages.
    ///
    /// To guarantee no messages are dropped, after calling `close()`, you must
    /// receive all items from the stream until `None` is returned.
    ///
    /// [`Permit`]: struct@tokio::sync::mpsc::Permit
    #[inline(always)]
    pub fn close(&mut self) {
        self.0.close();
    }
}

impl<T> Stream for ReceiverStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.0).poll(cx) {
            Poll::Ready(Ok(item)) => Poll::Ready(Some(item)),
            Poll::Ready(Err(_)) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> AsRef<Receiver<T>> for ReceiverStream<T> {
    fn as_ref(&self) -> &Receiver<T> {
        &self.0
    }
}

impl<T> AsMut<Receiver<T>> for ReceiverStream<T> {
    fn as_mut(&mut self) -> &mut Receiver<T> {
        &mut self.0
    }
}

impl<T> From<Receiver<T>> for ReceiverStream<T> {
    fn from(recv: Receiver<T>) -> Self {
        Self::new(recv)
    }
}
