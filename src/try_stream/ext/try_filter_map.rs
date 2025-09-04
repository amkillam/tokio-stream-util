#[cfg(feature = "sink")]
use async_sink::Sink;
use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::TryFuture;
use tokio_stream::Stream;

use super::{FusedStream, TryStream};

/// Stream for the [`try_filter_map`](super::TryStreamExt::try_filter_map)
/// method.
#[must_use = "streams do nothing unless polled"]
pub struct TryFilterMap<St, Fut, F> {
    stream: St,
    f: F,
    pending: Option<Fut>,
}

impl<St, Fut, F> fmt::Debug for TryFilterMap<St, Fut, F>
where
    St: fmt::Debug,
    Fut: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryFilterMap")
            .field("stream", &self.stream)
            .field("pending", &self.pending)
            .finish()
    }
}

impl<St, Fut, F> TryFilterMap<St, Fut, F> {
    pub(super) fn new(stream: St, f: F) -> Self {
        Self {
            stream,
            f,
            pending: None,
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

impl<St, Fut, F, T> FusedStream for TryFilterMap<St, Fut, F>
where
    St: TryStream + FusedStream,
    Fut: TryFuture<Ok = Option<T>, Error = St::Error>,
    F: FnMut(St::Ok) -> Fut,
{
    fn is_terminated(&self) -> bool {
        self.pending.is_none() && self.stream.is_terminated()
    }
}

impl<St, Fut, F, T> Stream for TryFilterMap<St, Fut, F>
where
    St: TryStream,
    Fut: TryFuture<Ok = Option<T>, Error = St::Error>,
    F: FnMut(St::Ok) -> Fut,
{
    type Item = Result<T, St::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };
        let mut pending = unsafe { Pin::new_unchecked(&mut this.pending) };
        let f = &mut this.f;

        loop {
            if let Some(p) = pending.as_mut().as_pin_mut() {
                // We have an item in progress, poll that until it's done
                let res = match p.try_poll(cx) {
                    Poll::Ready(res) => res,
                    Poll::Pending => return Poll::Pending,
                };
                pending.set(None);
                let item = match res {
                    Ok(item) => item,
                    Err(e) => return Poll::Ready(Some(Err(e))),
                };
                if let Some(val) = item {
                    return Poll::Ready(Some(Ok(val)));
                }
            } else {
                let next_from_stream = match stream.as_mut().try_poll_next(cx) {
                    Poll::Ready(r) => r,
                    Poll::Pending => return Poll::Pending,
                };

                match next_from_stream {
                    Some(Ok(item)) => {
                        // No item in progress, but the stream is still going
                        pending.set(Some(f(item)));
                    }
                    Some(Err(e)) => return Poll::Ready(Some(Err(e))),
                    None => {
                        // The stream is done
                        return Poll::Ready(None);
                    }
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let pending_len = if self.pending.is_some() { 1 } else { 0 };
        let (_, upper) = self.stream.size_hint();
        let upper = match upper {
            Some(x) => x.checked_add(pending_len),
            None => None,
        };
        (0, upper) // can't know a lower bound, due to the predicate
    }
}

// Forwarding impl of Sink from the underlying stream
#[cfg(feature = "sink")]
impl<St, Fut, F, Item> Sink<Item> for TryFilterMap<St, Fut, F>
where
    St: Sink<Item>,
{
    type Error = St::Error;

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
