use alloc::vec::Vec;
use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};

use tokio_stream::Stream;

use super::IntoFuseStream;
use crate::{FusedStream, TryStream};

/// Stream for the [`try_ready_chunks`](super::TryStreamExt::try_ready_chunks) method.
#[must_use = "streams do nothing unless polled"]
pub struct TryReadyChunks<St: TryStream> {
    stream: IntoFuseStream<St>,
    cap: usize, // https://github.com/rust-lang/futures-rs/issues/1475
    pending_error: Option<<St as TryStream>::Error>,
}

impl<St> fmt::Debug for TryReadyChunks<St>
where
    St: TryStream + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryReadyChunks")
            .field("stream", &self.stream)
            .field("cap", &self.cap)
            .finish()
    }
}

pub(super) struct TryReadyChunksProj<'pin, St: TryStream> {
    stream: Pin<&'pin mut IntoFuseStream<St>>,
    #[allow(dead_code)]
    cap: &'pin usize,
    #[allow(dead_code)]
    pending_error: &'pin mut Option<<St as TryStream>::Error>,
}

impl<St: TryStream + Unpin> Unpin for TryReadyChunks<St> {}

impl<St> TryReadyChunks<St>
where
    St: TryStream,
{
    pub(super) fn new(stream: St, capacity: usize) -> Self {
        assert!(capacity > 0);

        Self {
            stream: IntoFuseStream::new(stream),
            cap: capacity,
            pending_error: None,
        }
    }

    pub(super) fn project(self: Pin<&mut Self>) -> TryReadyChunksProj<'_, St> {
        unsafe {
            let this = self.get_unchecked_mut();
            TryReadyChunksProj {
                stream: Pin::new_unchecked(&mut this.stream),
                cap: &this.cap,
                pending_error: &mut this.pending_error,
            }
        }
    }
}

type TryReadyChunksStreamError<St> =
    TryReadyChunksError<<St as TryStream>::Ok, <St as TryStream>::Error>;

impl<St> Stream for TryReadyChunks<St>
where
    St: TryStream,
    IntoFuseStream<St>: Stream<Item = Result<St::Ok, St::Error>>,
{
    type Item = Result<Vec<St::Ok>, TryReadyChunksStreamError<St>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(err) = unsafe { self.as_mut().get_unchecked_mut() }
            .pending_error
            .take()
        {
            return Poll::Ready(Some(Err(TryReadyChunksError(Vec::new(), err))));
        }
        let cap = self.cap;
        let mut items: Vec<St::Ok> = Vec::new();

        loop {
            let this = self.as_mut();
            let stream = this.project().stream;
            match stream.poll_next(cx) {
                // Flush all the collected data if the underlying stream doesn't
                // contain more ready values
                Poll::Pending => {
                    return if items.is_empty() {
                        Poll::Pending
                    } else {
                        Poll::Ready(Some(Ok(items)))
                    };
                }

                // Push the ready item into the buffer and check whether it is full.
                // If so, return the buffer.
                Poll::Ready(Some(Ok(item))) => {
                    if items.is_empty() {
                        items.reserve_exact(cap);
                    }
                    items.push(item);
                    if items.len() >= cap {
                        break Poll::Ready(Some(Ok(items)));
                    }
                }

                // break the already collected items and the error.
                Poll::Ready(Some(Err(e))) => {
                    if items.is_empty() {
                        return Poll::Ready(Some(Err(TryReadyChunksError(items, e))));
                    } else {
                        // stash the error and yield the buffered items first
                        let this = unsafe { self.as_mut().get_unchecked_mut() };
                        this.pending_error = Some(e);
                        return Poll::Ready(Some(Ok(items)));
                    }
                }

                // Since the underlying stream ran out of values, break what we
                // have buffered, if we have anything.
                Poll::Ready(None) => {
                    let last = if items.is_empty() {
                        None
                    } else {
                        Some(Ok(items))
                    };
                    break Poll::Ready(last);
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.stream.size_hint();
        let lower = lower / self.cap;
        (lower, upper)
    }
}

impl<St> FusedStream for TryReadyChunks<St>
where
    St: TryStream,
    IntoFuseStream<St>: Stream<Item = Result<St::Ok, St::Error>> + FusedStream,
{
    fn is_terminated(&self) -> bool {
        FusedStream::is_terminated(&self.stream)
    }
}

/// Error indicating, that while chunk was collected inner stream produced an error.
///
/// Contains all items that were collected before an error occurred, and the stream error itself.
#[derive(PartialEq, Eq)]
pub struct TryReadyChunksError<T, E>(pub Vec<T>, pub E);

impl<T, E: fmt::Debug> fmt::Debug for TryReadyChunksError<T, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.1.fmt(f)
    }
}

impl<T, E: fmt::Display> fmt::Display for TryReadyChunksError<T, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.1.fmt(f)
    }
}

impl<T, E: fmt::Debug + fmt::Display> core::error::Error for TryReadyChunksError<T, E> {}

#[cfg(feature = "sink")]
use async_sink::Sink;
#[cfg(feature = "sink")]
// Forwarding impl of Sink from the underlying stream
impl<St, Item> Sink<Item> for TryReadyChunks<St>
where
    St: TryStream + Sink<Item>,
{
    type Error = <St as Sink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Forward Sink calls to the underlying St via IntoFuseStream::get_pin_mut()
        let stream = unsafe { self.map_unchecked_mut(|s| &mut s.stream) };
        stream.get_pin_mut().poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let stream = unsafe { self.map_unchecked_mut(|s| &mut s.stream) };
        stream.get_pin_mut().start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let stream = unsafe { self.map_unchecked_mut(|s| &mut s.stream) };
        stream.get_pin_mut().poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let stream = unsafe { self.map_unchecked_mut(|s| &mut s.stream) };
        stream.get_pin_mut().poll_close(cx)
    }
}
