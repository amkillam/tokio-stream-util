use super::{IntoFuseStream, TryStream};
use crate::{FusedStream, FuturesUnordered};
use core::{
    fmt,
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll},
};
use futures_core::future::TryFuture;
use futures_util::future::{IntoFuture, TryFutureExt};
use tokio_stream::Stream;

/// Stream for the
/// [`try_buffer_unordered`](super::TryStreamExt::try_buffer_unordered) method.
#[must_use = "streams do nothing unless polled"]
pub struct TryBufferUnordered<St>
where
    St: TryStream,
{
    stream: IntoFuseStream<St>,
    in_progress_queue: FuturesUnordered<IntoFuture<St::Ok>>,
    max: Option<NonZeroUsize>,
}

impl<St> fmt::Debug for TryBufferUnordered<St>
where
    St: TryStream + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryBufferUnordered")
            .field("stream", &self.stream)
            .field("in_progress_queue", &self.in_progress_queue)
            .field("max", &self.max)
            .finish()
    }
}

impl<St> TryBufferUnordered<St>
where
    St: TryStream,
{
    pub(super) fn new(stream: St, n: Option<usize>) -> Self {
        Self {
            stream: IntoFuseStream::new(stream),
            in_progress_queue: FuturesUnordered::new(),
            max: n.and_then(NonZeroUsize::new),
        }
    }
}

impl<St> Stream for TryBufferUnordered<St>
where
    St: TryStream,
    St::Ok: TryFuture<Error = St::Error>,
{
    type Item = Result<<St::Ok as TryFuture>::Ok, St::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };
        let in_progress_queue_len = this.in_progress_queue.len();
        let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };

        // First up, try to spawn off as many futures as possible by filling up
        // our queue of futures. Propagate errors from the stream immediately.
        while this
            .max
            .map(|max| in_progress_queue_len < max.get())
            .unwrap_or(true)
        {
            match stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(fut))) => {
                    this.in_progress_queue.push(fut.into_future());
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Ready(None) | Poll::Pending => break,
            }
        }

        // Attempt to pull the next value from the in_progress_queue
        match unsafe { Pin::new_unchecked(&mut this.in_progress_queue) }.poll_next(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Some(item)) => return Poll::Ready(Some(item)),
            Poll::Ready(None) => {}
        }

        // If more values are still coming from the stream, we're not done yet
        if stream.is_terminated() {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

#[cfg(feature = "sink")]
use tokio_sink::Sink;
#[cfg(feature = "sink")]
// Forwarding impl of Sink from the underlying stream
impl<St, Item> Sink<Item> for TryBufferUnordered<St>
where
    St: TryStream + Sink<Item>,
    St::Ok: TryFuture<Error = St::Error>,
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
