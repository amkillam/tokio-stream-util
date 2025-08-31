use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};

use super::{FusedStream, TryStream};

use tokio_stream::Stream;

/// Stream for the [`try_flatten`](super::TryStreamExt::try_flatten) method.
#[must_use = "streams do nothing unless polled"]
pub struct TryFlatten<St>
where
    St: TryStream,
{
    stream: St,
    next: Option<St::Ok>,
}

impl<St> Unpin for TryFlatten<St>
where
    St: TryStream + Unpin,
    St::Ok: Unpin,
{
}

impl<St: TryStream + fmt::Debug> fmt::Debug for TryFlatten<St>
where
    St::Ok: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryFlatten")
            .field("stream", &self.stream)
            .field("next", &self.next)
            .finish()
    }
}

impl<St> TryFlatten<St>
where
    St: TryStream,
    St::Ok: TryStream,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    pub(super) fn new(stream: St) -> Self {
        Self { stream, next: None }
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
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) }
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> St {
        self.stream
    }
}

impl<St> FusedStream for TryFlatten<St>
where
    St: TryStream + FusedStream,
    St::Ok: TryStream,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    fn is_terminated(&self) -> bool {
        self.next.is_none() && self.stream.is_terminated()
    }
}

impl<St> Stream for TryFlatten<St>
where
    St: TryStream,
    St::Ok: TryStream,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    type Item = Result<<St::Ok as TryStream>::Ok, <St::Ok as TryStream>::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };
        let mut next = unsafe { Pin::new_unchecked(&mut this.next) };

        Poll::Ready(loop {
            if let Some(s) = next.as_mut().as_pin_mut() {
                let poll_res = match s.try_poll_next(cx) {
                    Poll::Ready(res) => res,
                    Poll::Pending => return Poll::Pending,
                };

                let item = match poll_res {
                    Some(Ok(item)) => Some(item),
                    Some(Err(e)) => {
                        next.set(None);
                        break Some(Err(e));
                    }
                    None => {
                        next.set(None);
                        None
                    }
                };

                if let Some(item) = item {
                    break Some(Ok(item));
                }
            } else {
                let poll_res = match stream.as_mut().try_poll_next(cx) {
                    Poll::Ready(res) => res,
                    Poll::Pending => return Poll::Pending,
                };

                let s = match poll_res {
                    Some(Ok(s)) => Some(s),
                    Some(Err(e)) => break Some(Err(e.into())),
                    None => break None,
                };

                if let Some(s) = s {
                    next.set(Some(s));
                }
            }
        })
    }
}
