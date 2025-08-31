use super::IntoFuseStream;
use super::{FusedStream, TryStream};
use crate::FuturesOrdered;
use core::num::NonZeroUsize;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::TryFuture;
use futures_util::future::{IntoFuture, TryFutureExt};

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

pub(crate) struct TryBufferedProj<'pin, St>
where
    St: TryStream,
    St::Ok: TryFuture,
{
    stream: Pin<&'pin mut IntoFuseStream<St>>,
    #[allow(dead_code)]
    in_progress_queue: &'pin mut FuturesOrdered<IntoFuture<St::Ok>>,
    #[allow(dead_code)]
    max: &'pin Option<NonZeroUsize>,
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

    pub(crate) fn project(self: Pin<&mut Self>) -> TryBufferedProj<'_, St> {
        unsafe {
            let this = self.get_unchecked_mut();
            TryBufferedProj {
                stream: Pin::new_unchecked(&mut this.stream),
                in_progress_queue: &mut this.in_progress_queue,
                max: &this.max,
            }
        }
    }

    /// Acquires a pinned mutable reference to the underlying stream that this
    /// combinator is pulling from.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        self.project().stream.get_pin_mut()
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &St {
        self.stream.get_ref()
    }

    /// Acquires a mutable reference to the underlying stream that this
    /// combinator is pulling from.
    pub fn get_mut(&mut self) -> &mut St {
        self.stream.get_mut()
    }

    /// Returns the number of chunk-generating futures that are currently
    /// being queued and being polled.
    pub fn len(&self) -> usize {
        self.in_progress_queue.len()
    }

    /// Returns `true` if there are no chunk-generating futures currently
    /// being queued and polled.
    pub fn is_empty(&self) -> bool {
        self.in_progress_queue.is_empty()
    }
}

impl<St> FusedStream for TryBuffered<St>
where
    St: TryStream,
    St::Ok: TryFuture<Error = St::Error>,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated() && self.in_progress_queue.is_empty()
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

#[cfg(feature = "sink")]
use tokio_sink::Sink;
#[cfg(feature = "sink")]
// Forwarding impl of Sink from the underlying stream
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
