use super::IntoFuseStream;
use super::{FusedStream, TryStream};
use crate::FuturesOrdered;
use core::num::NonZeroUsize;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::TryFuture;
use futures_util::future::{IntoFuture, TryFutureExt};
#[cfg(feature = "sink")]
use tokio_sink::Sink;
use tokio_stream::Stream;

/// Stream for the [`try_buffered`](super::TryStreamExt::try_buffered) method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct TryBuffered<St>
where
    St: TryStream,
    St::Ok: TryFuture,
{
    stream: IntoFuseStream<St>,
    in_progress_queue: FuturesOrdered<IntoFuture<St::Ok>>,
    max: Option<NonZeroUsize>,
}

impl<St> TryBuffered<St>
where
    St: TryStream,
    St::Ok: TryFuture,
{
    pub(super) fn new(stream: St, n: Option<usize>) -> Self {
        Self {
            stream: IntoFuseStream::new(stream),
            in_progress_queue: FuturesOrdered::new(),
            max: n.and_then(NonZeroUsize::new),
        }
    }
}

impl<St> Stream for TryBuffered<St>
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
                    this.in_progress_queue.push_back(fut.into_future());
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Ready(None) | Poll::Pending => break,
            }
        }

        // Attempt to pull the next value from the in_progress_queue
        match Pin::new(&mut this.in_progress_queue).poll_next(cx) {
            x @ (Poll::Pending | Poll::Ready(Some(_))) => return x,
            Poll::Ready(None) => {}
        }

        // If more values are still coming from the stream, we're not done yet
        if FusedStream::is_terminated(&stream) {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

// Forwarding impl of Sink from the underlying stream
#[cfg(feature = "sink")]
impl<S, Item, E> Sink<Item> for TryBuffered<S>
where
    S: TryStream + Sink<Item, Error = E>,
    S::Ok: TryFuture<Error = E>,
{
    type Error = E;

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
