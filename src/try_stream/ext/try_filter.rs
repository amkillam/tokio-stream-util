use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::Future;
#[cfg(feature = "sink")]
use tokio_sink::Sink;
use tokio_stream::Stream;

use super::{FusedStream, TryStream};

/// Stream for the [`try_filter`](super::TryStreamExt::try_filter)
/// method.
#[must_use = "streams do nothing unless polled"]
pub struct TryFilter<St, Fut, F>
where
    St: TryStream,
{
    stream: St,
    f: F,
    pending_fut: Option<Fut>,
    pending_item: Option<St::Ok>,
}

impl<St, Fut, F> Unpin for TryFilter<St, Fut, F>
where
    St: TryStream + Unpin,
    Fut: Unpin,
{
}

impl<St, Fut, F> TryFilter<St, Fut, F>
where
    St: TryStream,
{
    // Safety: `get_unchecked_mut` is fine because we don't move anything.
    // We can use `new_unchecked` because the `inner` parts are guaranteed
    // to be pinned, as they come from `self` which is pinned, and we never
    // offer an unpinned `&mut A` or `&mut B` through `Pin<&mut Self>`. We
    // also don't have an implementation of `Drop`, nor manual `Unpin`.
    unsafe fn project(
        self: Pin<&mut Self>,
    ) -> (
        Pin<&mut St>,
        &mut F,
        Pin<&mut Option<Fut>>,
        &mut Option<St::Ok>,
    ) {
        let this = self.get_unchecked_mut();
        (
            Pin::new_unchecked(&mut this.stream),
            &mut this.f,
            Pin::new_unchecked(&mut this.pending_fut),
            &mut this.pending_item,
        )
    }
}

impl<St, Fut, F> fmt::Debug for TryFilter<St, Fut, F>
where
    St: TryStream + fmt::Debug,
    St::Ok: fmt::Debug,
    Fut: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryFilter")
            .field("stream", &self.stream)
            .field("pending_fut", &self.pending_fut)
            .field("pending_item", &self.pending_item)
            .finish()
    }
}

impl<St, Fut, F> TryFilter<St, Fut, F>
where
    St: TryStream,
{
    pub(super) fn new(stream: St, f: F) -> Self {
        Self {
            stream,
            f,
            pending_fut: None,
            pending_item: None,
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

impl<St, Fut, F> FusedStream for TryFilter<St, Fut, F>
where
    St: TryStream + FusedStream,
    F: FnMut(&St::Ok) -> Fut,
    Fut: Future<Output = bool>,
{
    fn is_terminated(&self) -> bool {
        self.pending_fut.is_none() && self.stream.is_terminated()
    }
}

impl<St, Fut, F> Stream for TryFilter<St, Fut, F>
where
    St: TryStream,
    Fut: Future<Output = bool>,
    F: FnMut(&St::Ok) -> Fut,
{
    type Item = Result<St::Ok, St::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (mut stream, f, mut pending_fut, pending_item) = unsafe { self.project() };

        loop {
            if let Some(fut) = pending_fut.as_mut().as_pin_mut() {
                let res = match fut.poll(cx) {
                    Poll::Ready(res) => res,
                    Poll::Pending => return Poll::Pending,
                };
                pending_fut.set(None);
                if res {
                    return Poll::Ready(pending_item.take().map(Ok));
                }
                *pending_item = None;
            } else {
                let item_res = match stream.as_mut().try_poll_next(cx) {
                    Poll::Ready(item_res) => item_res,
                    Poll::Pending => return Poll::Pending,
                };

                if let Some(item) = match item_res {
                    Some(Ok(item)) => Some(item),
                    Some(Err(e)) => return Poll::Ready(Some(Err(e))),
                    None => None,
                } {
                    pending_fut.set(Some(f(&item)));
                    *pending_item = Some(item);
                } else {
                    return Poll::Ready(None);
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let pending_len = if self.pending_item.is_some() { 1 } else { 0 };
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
impl<St, Fut, F, Item, E> Sink<Item> for TryFilter<St, Fut, F>
where
    St: TryStream + Sink<Item, Error = E>,
{
    type Error = E;

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
