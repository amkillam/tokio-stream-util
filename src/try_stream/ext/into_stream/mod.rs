pub(crate) mod fused;
pub(crate) use fused::IntoFuseStream;

use super::{FusedStream, TryStream};
use core::pin::Pin;
use core::task::{Context, Poll};
#[cfg(feature = "sink")]
use tokio_sink::Sink;
use tokio_stream::Stream;

/// Stream for the [`into_stream`](super::TryStreamExt::into_stream) method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct IntoStream<St> {
    stream: St,
}

struct IntoStreamProj<'pin, St> {
    stream: Pin<&'pin mut St>,
}

impl<St: Unpin> Unpin for IntoStream<St> {}

impl<St> IntoStream<St> {
    #[inline]
    pub(super) fn new(stream: St) -> Self {
        Self { stream }
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &St {
        &self.stream
    }

    /// Acquires a mutable reference to the underlying stream that this
    /// combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut St {
        &mut self.stream
    }

    /// Acquires a pinned mutable reference to the underlying stream that this
    /// combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        unsafe { self.map_unchecked_mut(|s| &mut s.stream) }
    }

    pub(crate) fn project(self: Pin<&mut Self>) -> IntoStreamProj<'_, St> {
        unsafe {
            let this = self.get_unchecked_mut();
            IntoStreamProj {
                stream: Pin::new_unchecked(&mut this.stream),
            }
        }
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> St {
        self.stream
    }
}

impl<St: TryStream + FusedStream> FusedStream for IntoStream<St> {
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

impl<St: TryStream> Stream for IntoStream<St> {
    type Item = Result<St::Ok, St::Error>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_pin_mut().try_poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

// Forwarding impl of Sink from the underlying stream
#[cfg(feature = "sink")]
impl<St, Item> Sink<Item> for IntoStream<St>
where
    St: Sink<Item>,
{
    type Error = St::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_pin_mut().poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        self.get_pin_mut().start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_pin_mut().poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.get_pin_mut().poll_close(cx)
    }
}
