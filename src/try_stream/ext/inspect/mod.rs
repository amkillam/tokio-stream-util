pub mod inspect_err;
pub mod inspect_ok;
use core::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_stream::Stream;

use crate::FusedStream;

/// `Inspect` stream adapter
///
/// This is the general-purpose `Inspect` adapter used by both `inspect_ok`
/// and `inspect_err`.
pub struct Inspect<St, F> {
    stream: St,
    f: F,
}

impl<St, F> Inspect<St, F> {
    pub fn new(stream: St, f: F) -> Self {
        Inspect { stream, f }
    }

    /// Mutable access to the inner stream.
    pub fn get_mut(&mut self) -> &mut St {
        &mut self.stream
    }

    /// Pinned mutable access to the inner stream.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).stream) }
    }

    /// Consume adapter and return the inner stream.
    pub fn into_inner(self) -> St {
        self.stream
    }
}

impl<St, F> fmt::Debug for Inspect<St, F>
where
    St: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inspect")
            .field("stream", &self.stream)
            .finish()
    }
}

impl<St, F, T> Stream for Inspect<St, F>
where
    St: Stream<Item = T>,
    F: FnMut(&T),
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        // Project to fields
        let this = unsafe { Pin::get_unchecked_mut(self) };
        let mut inner = unsafe { Pin::new_unchecked(&mut this.stream) };

        match inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(item)) => {
                // call inspector on a borrowed reference
                (this.f)(&item);
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

impl<St, F> FusedStream for Inspect<St, F>
where
    St: FusedStream + Stream,
    F: FnMut(&St::Item),
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}
