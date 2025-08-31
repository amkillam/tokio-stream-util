use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::TryFuture;
use tokio_stream::Stream;

use super::{FusedStream, TryStream};

/// Stream for the [`try_skip_while`](super::TryStreamExt::try_skip_while)
/// method.
#[must_use = "streams do nothing unless polled"]
pub struct TrySkipWhile<St, Fut, F>
where
    St: TryStream,
{
    stream: St,
    f: F,
    pending_fut: Option<Fut>,
    pending_item: Option<St::Ok>,
    done_skipping: bool,
}

impl<St, Fut, F> Unpin for TrySkipWhile<St, Fut, F>
where
    St: TryStream + Unpin,
    Fut: Unpin,
{
}

impl<St, Fut, F> fmt::Debug for TrySkipWhile<St, Fut, F>
where
    St: TryStream + fmt::Debug,
    St::Ok: fmt::Debug,
    Fut: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrySkipWhile")
            .field("stream", &self.stream)
            .field("pending_fut", &self.pending_fut)
            .field("pending_item", &self.pending_item)
            .field("done_skipping", &self.done_skipping)
            .finish()
    }
}

impl<St, Fut, F> TrySkipWhile<St, Fut, F>
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
            done_skipping: false,
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

impl<St, Fut, F> Stream for TrySkipWhile<St, Fut, F>
where
    St: TryStream,
    F: FnMut(&St::Ok) -> Fut,
    Fut: TryFuture<Ok = bool, Error = St::Error>,
{
    type Item = Result<St::Ok, St::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };

        if this.done_skipping {
            let stream = unsafe { Pin::new_unchecked(&mut this.stream) };
            return stream.try_poll_next(cx);
        }

        loop {
            if this.pending_fut.is_some() {
                let mut fut = unsafe { Pin::new_unchecked(this.pending_fut.as_mut().unwrap()) };
                let skipped = match fut.as_mut().try_poll(cx) {
                    Poll::Ready(Ok(skipped)) => skipped,
                    Poll::Ready(Err(e)) => {
                        this.done_skipping = true;
                        this.pending_fut = None;
                        this.pending_item = None;
                        return Poll::Ready(Some(Err(e)));
                    }
                    Poll::Pending => return Poll::Pending,
                };

                this.pending_fut = None;
                let item = this.pending_item.take();

                if !skipped {
                    this.done_skipping = true;
                    return Poll::Ready(item.map(Ok));
                }
            } else {
                let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };
                match stream.as_mut().try_poll_next(cx) {
                    Poll::Ready(Some(Ok(item))) => {
                        this.pending_fut = Some((this.f)(&item));
                        this.pending_item = Some(item);
                    }
                    Poll::Ready(Some(Err(e))) => {
                        this.done_skipping = true;
                        return Poll::Ready(Some(Err(e)));
                    }
                    Poll::Ready(None) => {
                        this.done_skipping = true;
                        return Poll::Ready(None);
                    }
                    Poll::Pending => return Poll::Pending,
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let pending_len = usize::from(self.pending_item.is_some());
        let (_, upper) = self.stream.size_hint();
        let upper = match upper {
            Some(x) => x.checked_add(pending_len),
            None => None,
        };
        (0, upper) // can't know a lower bound, due to the predicate
    }
}

impl<St, Fut, F> FusedStream for TrySkipWhile<St, Fut, F>
where
    St: TryStream + FusedStream,
    F: FnMut(&St::Ok) -> Fut,
    Fut: TryFuture<Ok = bool, Error = St::Error>,
{
    fn is_terminated(&self) -> bool {
        self.pending_item.is_none() && self.stream.is_terminated()
    }
}
