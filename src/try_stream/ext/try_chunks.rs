use alloc::vec::Vec;
use core::fmt;
use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll};

#[cfg(feature = "sink")]
use tokio_sink::Sink;
use tokio_stream::Stream;

use super::IntoFuseStream;

use super::{FusedStream, TryStream};

/// Stream for the [`try_chunks`](super::TryStreamExt::try_chunks) method.
#[must_use = "streams do nothing unless polled"]
pub struct TryChunks<St: TryStream> {
    stream: IntoFuseStream<St>,
    items: Vec<St::Ok>,
    cap: usize,
    pending_error: Option<St::Error>,
}

impl<St> fmt::Debug for TryChunks<St>
where
    St: TryStream + fmt::Debug,
    St::Ok: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryChunks")
            .field("stream", &self.stream)
            .field("items", &self.items)
            .field("cap", &self.cap)
            .finish()
    }
}

struct TryChunksProj<'pin, St: TryStream> {
    stream: Pin<&'pin mut IntoFuseStream<St>>,
    #[allow(dead_code)]
    items: &'pin mut Vec<St::Ok>,
    #[allow(dead_code)]
    cap: &'pin usize,
}

impl<St: TryStream + Unpin> Unpin for TryChunks<St> {}

impl<St: TryStream> TryChunks<St> {
    pub(super) fn new(stream: St, capacity: usize) -> Self {
        assert!(capacity > 0);

        Self {
            stream: IntoFuseStream::new(stream),
            items: Vec::with_capacity(capacity),
            cap: capacity,
            pending_error: None,
        }
    }

    fn take(self: Pin<&mut Self>) -> Vec<St::Ok> {
        let this = unsafe { self.get_unchecked_mut() };
        let cap = this.cap;
        mem::replace(&mut this.items, Vec::with_capacity(cap))
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &St {
        self.stream.get_ref()
    }

    /// Acquires a mutable reference to the underlying stream that this
    /// combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut St {
        self.stream.get_mut()
    }

    /// Acquires a pinned mutable reference to the underlying stream that this
    /// combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        self.project().stream.get_pin_mut()
    }

    fn project<'pin>(self: Pin<&'pin mut Self>) -> TryChunksProj<'pin, St> {
        unsafe {
            let this = self.get_unchecked_mut();
            TryChunksProj {
                stream: Pin::new_unchecked(&mut this.stream),
                items: &mut this.items,
                cap: &this.cap,
            }
        }
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> St {
        self.stream.into_inner()
    }
}

type TryChunksStreamError<St> = TryChunksError<<St as TryStream>::Ok, <St as TryStream>::Error>;

impl<St: TryStream> Stream for TryChunks<St> {
    type Item = Result<Vec<St::Ok>, TryChunksStreamError<St>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(err) = unsafe { self.as_mut().get_unchecked_mut() }
            .pending_error
            .take()
        {
            return Poll::Ready(Some(Err(TryChunksError(Vec::new(), err))));
        }
        loop {
            let this = self.as_mut();
            let stream = this.project().stream;
            match stream.poll_next(cx) {
                // Push the item into the buffer and check whether it is full.
                // If so, replace our buffer with a new and empty one and return
                // the full one.
                Poll::Ready(Some(Ok(item))) => {
                    let this = unsafe { self.as_mut().get_unchecked_mut() };
                    this.items.push(item);
                    if this.items.len() >= this.cap {
                        let items = mem::replace(&mut this.items, Vec::with_capacity(this.cap));
                        break Poll::Ready(Some(Ok(items)));
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    let this = unsafe { self.as_mut().get_unchecked_mut() };
                    if this.items.is_empty() {
                        break Poll::Ready(Some(Err(TryChunksError(Vec::new(), e))));
                    } else {
                        // stash error and yield the buffered items first
                        this.pending_error = Some(e);
                        let items = mem::replace(&mut this.items, Vec::with_capacity(this.cap));
                        break Poll::Ready(Some(Ok(items)));
                    }
                }

                // Since the underlying stream ran out of values, break what we
                // have buffered, if we have anything.
                Poll::Ready(None) => {
                    let this = unsafe { self.as_mut().get_unchecked_mut() };
                    let last = if this.items.is_empty() {
                        None
                    } else {
                        Some(mem::take(&mut this.items))
                    };

                    break Poll::Ready(last.map(Ok));
                }
                Poll::Pending => {
                    break if self.items.is_empty() {
                        Poll::Pending
                    } else {
                        let items = self.take();
                        Poll::Ready(Some(Ok(items)))
                    }
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let chunk_len = if !self.items.is_empty() { 1 } else { 0 };
        let (lower, upper) = self.stream.size_hint();
        let lower = (lower / self.cap).saturating_add(chunk_len);
        let upper = match upper {
            Some(x) => x.checked_add(chunk_len),
            None => None,
        };
        (lower, upper)
    }
}

impl<St: TryStream> FusedStream for TryChunks<St> {
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated() && self.items.is_empty()
    }
}

// Forwarding impl of Sink from the underlying stream
#[cfg(feature = "sink")]
impl<St, Item> Sink<Item> for TryChunks<St>
where
    St: TryStream + Sink<Item>,
{
    type Error = <St as Sink<Item>>::Error;

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

/// Error indicating, that while chunk was collected inner stream produced an error.
///
/// Contains all items that were collected before an error occurred, and the stream error itself.
#[derive(PartialEq, Eq)]
pub struct TryChunksError<T, E>(pub Vec<T>, pub E);

impl<T, E: fmt::Debug> fmt::Debug for TryChunksError<T, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.1.fmt(f)
    }
}

impl<T, E: fmt::Display> fmt::Display for TryChunksError<T, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.1.fmt(f)
    }
}

impl<T, E: fmt::Debug + fmt::Display> core::error::Error for TryChunksError<T, E> {}
