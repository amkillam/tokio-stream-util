use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::TryFuture;
#[cfg(feature = "sink")]
use tokio_sink::Sink;
use tokio_stream::Stream;

use super::{FusedStream, TryStream};

/// Stream for the [`try_take_while`](super::TryStreamExt::try_take_while)
/// method.
#[must_use = "streams do nothing unless polled"]
pub struct TryTakeWhile<St, Fut, F>
where
    St: TryStream,
{
    stream: St,
    f: F,
    pending_fut: Option<Fut>,
    pending_item: Option<St::Ok>,
    done_taking: bool,
}

impl<St, Fut, F> Unpin for TryTakeWhile<St, Fut, F>
where
    St: TryStream + Unpin,
    Fut: Unpin,
{
}

impl<St, Fut, F> fmt::Debug for TryTakeWhile<St, Fut, F>
where
    St: TryStream + fmt::Debug,
    St::Ok: fmt::Debug,
    Fut: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryTakeWhile")
            .field("stream", &self.stream)
            .field("pending_fut", &self.pending_fut)
            .field("pending_item", &self.pending_item)
            .field("done_taking", &self.done_taking)
            .finish()
    }
}

impl<St, Fut, F> TryTakeWhile<St, Fut, F>
where
    St: TryStream,
    F: FnMut(&St::Ok) -> Fut,
    Fut: TryFuture<Ok = bool, Error = St::Error>,
{
    pub(super) fn new(stream: St, f: F) -> Self {
        Self {
            stream,
            f,
            pending_fut: None,
            pending_item: None,
            done_taking: false,
        }
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

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> St {
        self.stream
    }
}

impl<St, Fut, F> Stream for TryTakeWhile<St, Fut, F>
where
    St: TryStream,
    F: FnMut(&St::Ok) -> Fut,
    Fut: TryFuture<Ok = bool, Error = St::Error>,
{
    type Item = Result<St::Ok, St::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };

        if this.done_taking {
            return Poll::Ready(None);
        }

        loop {
            if let Some(fut) = &mut this.pending_fut {
                let fut = unsafe { Pin::new_unchecked(fut) };
                let take = match fut.try_poll(cx) {
                    Poll::Ready(Ok(take)) => take,
                    Poll::Ready(Err(e)) => {
                        this.done_taking = true;
                        this.pending_fut = None;
                        this.pending_item = None;
                        return Poll::Ready(Some(Err(e)));
                    }
                    Poll::Pending => return Poll::Pending,
                };

                this.pending_fut = None;
                let item = this.pending_item.take();

                if take {
                    return Poll::Ready(item.map(Ok));
                } else {
                    this.done_taking = true;
                    return Poll::Ready(None);
                }
            } else {
                let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };
                match stream.as_mut().try_poll_next(cx) {
                    Poll::Ready(Some(Ok(item))) => {
                        this.pending_fut = Some((this.f)(&item));
                        this.pending_item = Some(item);
                    }
                    Poll::Ready(Some(Err(e))) => {
                        this.done_taking = true;
                        return Poll::Ready(Some(Err(e)));
                    }
                    Poll::Ready(None) => {
                        this.done_taking = true;
                        return Poll::Ready(None);
                    }
                    Poll::Pending => return Poll::Pending,
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.done_taking {
            return (0, Some(0));
        }

        let pending_len = usize::from(self.pending_item.is_some());
        let (_, upper) = self.stream.size_hint();
        let upper = match upper {
            Some(x) => x.checked_add(pending_len),
            None => None,
        };
        (0, upper) // can't know a lower bound, due to the predicate
    }
}

impl<St, Fut, F> FusedStream for TryTakeWhile<St, Fut, F>
where
    St: TryStream + FusedStream,
    F: FnMut(&St::Ok) -> Fut,
    Fut: TryFuture<Ok = bool, Error = St::Error>,
{
    fn is_terminated(&self) -> bool {
        self.done_taking || self.pending_item.is_none() && self.stream.is_terminated()
    }
}

// Forwarding impl of Sink from the underlying stream
#[cfg(feature = "sink")]
impl<St, Fut, F, Item> Sink<Item> for TryTakeWhile<St, Fut, F>
where
    St: TryStream + Sink<Item>,
    Fut: TryFuture,
{
    type Error = <St as Sink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.stream) }.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        unsafe { self.map_unchecked_mut(|s| &mut s.stream) }.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.stream) }.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.stream) }.poll_close(cx)
    }
}
