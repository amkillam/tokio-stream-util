use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::TryFuture;
use super::{FusedStream, TryStream};
use tokio_stream::Stream;
#[cfg(feature = "sink")]
use tokio_sink::Sink;

/// Stream for the [`and_then`](super::TryStreamExt::and_then) method.
#[must_use = "streams do nothing unless polled"]
pub struct AndThen<St, Fut, F> {
    stream: St,
    future: Option<Fut>,
    f: F,
}

impl<St, Fut, F> fmt::Debug for AndThen<St, Fut, F>
where
    St: fmt::Debug,
    Fut: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AndThen")
            .field("stream", &self.stream)
            .field("future", &self.future)
            .finish()
    }
}

impl<St, Fut, F> AndThen<St, Fut, F>
where
    St: TryStream,
    F: FnMut(St::Ok) -> Fut,
    Fut: TryFuture<Error = St::Error>,
{
    pub(super) fn new(stream: St, f: F) -> Self {
        Self { stream, future: None, f }
    }
}

impl<St, Fut, F> AndThen<St, Fut, F> {
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

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> St {
        self.stream
    }
}

impl<St, Fut, F> Stream for AndThen<St, Fut, F>
where
    St: TryStream,
    F: FnMut(St::Ok) -> Fut,
    Fut: TryFuture<Error = St::Error>,
{
    type Item = Result<Fut::Ok, St::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };
        let mut future = unsafe { Pin::new_unchecked(&mut this.future) };
        let f = &mut this.f;

        loop {
            if let Some(fut) = future.as_mut().as_pin_mut() {
                let item = match fut.try_poll(cx) {
                    Poll::Ready(result) => result,
                    Poll::Pending => return Poll::Pending,
                };
                future.set(None);
                return Poll::Ready(Some(item));
            }

            let next_item_res = match stream.as_mut().try_poll_next(cx) {
                Poll::Ready(res) => res,
                Poll::Pending => return Poll::Pending,
            };

            match next_item_res {
                Some(Ok(item)) => {
                    future.set(Some(f(item)));
                }
                Some(Err(e)) => return Poll::Ready(Some(Err(e))),
                None => return Poll::Ready(None),
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let future_len = if self.future.is_some() { 1 } else { 0 };
        let (lower, upper) = self.stream.size_hint();
        let lower = lower.saturating_add(future_len);
        let upper = match upper {
            Some(x) => x.checked_add(future_len),
            None => None,
        };
        (lower, upper)
    }
}

impl<St, Fut, F> FusedStream for AndThen<St, Fut, F>
where
    St: TryStream + FusedStream,
    F: FnMut(St::Ok) -> Fut,
    Fut: TryFuture<Error = St::Error>,
{
    fn is_terminated(&self) -> bool {
        self.future.is_none() && self.stream.is_terminated()
    }
}

// Forwarding impl of Sink from the underlying stream
#[cfg(feature = "sink")]
impl<St, Fut, F, Item> Sink<Item> for AndThen<St, Fut, F>
where
    St: Sink<Item>,
{
    type Error = St::Error;

    fn poll_ready(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) }.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) }.start_send(item)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) }.poll_flush(cx)
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) }.poll_close(cx)
    }
}
