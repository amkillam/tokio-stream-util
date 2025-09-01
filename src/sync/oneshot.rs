//! A wrapper around [`tokio::sync::oneshot::Receiver`] that implements [`Stream`].
//!
//! ! [`tokio::sync::oneshot::Receiver`]: struct@tokio::sync::oneshot::Receiver
//! ! [`Stream`]: trait@tokio_stream::Stream
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use tokio_stream::Stream;

/// A wrapper around [`tokio::sync::oneshot::Receiver`] that implements [`Stream`].
///
/// [`tokio::sync::oneshot::Receiver`]: struct@tokio::sync::oneshot::Receiver
/// [`Stream`]: trait@crate::Stream
#[derive(Debug)]
#[repr(transparent)]
pub struct Receiver<T>(pub oneshot::Receiver<T>);

pub(crate) struct ReceiverProj<'pin, T>(Pin<&'pin mut oneshot::Receiver<T>>);

impl<T> Receiver<T> {
    /// Create a new `Receiver`.
    #[inline(always)]
    pub fn new(recv: oneshot::Receiver<T>) -> Self {
        Self(recv)
    }

    /// Get back the inner `Receiver`.
    #[inline(always)]
    pub fn into_inner(self) -> oneshot::Receiver<T> {
        self.0
    }

    pub(crate) fn project<'pin>(self: Pin<&'pin mut Self>) -> ReceiverProj<'pin, T> {
        let this = self.get_mut();
        ReceiverProj(Pin::new(&mut this.0))
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

    /// Forwards to the inner receiver's [`is_terminated`] method.
    ///
    /// [`is_terminated`]: tokio::sync::oneshot::Receiver::is_terminated
    #[inline(always)]
    pub fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }

    /// Forwards to the inner receiver's [`is_empty`] method.
    ///
    /// [`is_empty`]: tokio::sync::oneshot::Receiver::is_empty
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Forwards to the inner receiver's [`try_recv`] method.
    ///
    /// [`try_recv`]: tokio::sync::oneshot::Receiver::try_recv
    #[inline(always)]
    pub fn try_recv(&mut self) -> Result<T, oneshot::error::TryRecvError> {
        self.0.try_recv()
    }

    /// Forwards to the inner receiver's [`blocking_recv`] method.
    ///
    /// [`blocking_recv`]: tokio::sync::oneshot::Receiver::blocking_recv
    #[inline(always)]
    pub fn blocking_recv(self) -> Result<T, oneshot::error::RecvError> {
        self.0.blocking_recv()
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.0).poll(cx) {
            Poll::Ready(Ok(item)) => Poll::Ready(Some(item)),
            Poll::Ready(Err(_)) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = <oneshot::Receiver<T> as Future>::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().0.poll(cx)
    }
}

impl<T> AsRef<oneshot::Receiver<T>> for Receiver<T> {
    fn as_ref(&self) -> &oneshot::Receiver<T> {
        &self.0
    }
}

impl<T> AsMut<oneshot::Receiver<T>> for Receiver<T> {
    fn as_mut(&mut self) -> &mut oneshot::Receiver<T> {
        &mut self.0
    }
}

impl<T> From<oneshot::Receiver<T>> for Receiver<T> {
    fn from(recv: oneshot::Receiver<T>) -> Self {
        Self::new(recv)
    }
}
