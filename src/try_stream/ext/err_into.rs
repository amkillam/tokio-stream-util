use super::TryStream;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::Stream;

/// Stream for the [`err_into`](super::TryStreamExt::err_into) method.
#[derive(Clone)]
#[must_use = "streams do nothing unless polled"]
pub struct ErrInto<St, E> {
    stream: St,
    _phantom: PhantomData<E>,
}

impl<St, E> ErrInto<St, E> {
    pub(super) fn new(stream: St) -> Self {
        Self {
            stream,
            _phantom: PhantomData,
        }
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    ///
    /// SAFETY: The returned reference is valid as long as `self` is
    /// valid.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        // SAFETY: `stream` is pinned because it is inside a `Pin<ErrInto>`.
        unsafe { self.map_unchecked_mut(|s| &mut s.stream) }
    }
}

impl<St, E> Stream for ErrInto<St, E>
where
    St: TryStream,
    St::Error: Into<E>,
{
    type Item = Result<St::Ok, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_pin_mut().try_poll_next(cx) {
            Poll::Ready(Some(Ok(ok))) => Poll::Ready(Some(Ok(ok))),
            Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(err.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(feature = "sink")]
use tokio_sink::Sink;
#[cfg(feature = "sink")]
impl<St, E, Item> tokio_sink::Sink<Item> for ErrInto<St, E>
where
    St: Sink<Item>,
    St::Error: Into<E>,
{
    type Error = E;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.get_pin_mut().poll_ready(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        match self.get_pin_mut().start_send(item) {
            Ok(()) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.get_pin_mut().poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.get_pin_mut().poll_close(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => Poll::Pending,
        }
    }
}
