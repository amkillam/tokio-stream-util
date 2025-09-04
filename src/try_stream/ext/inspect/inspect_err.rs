use crate::try_stream::IntoStream;

use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::{FusedStream, TryStream};

/// Stream for the [`inspect_err`](crate::TryStreamExt::inspect_err) method.
pub struct InspectErr<St, F> {
    stream: IntoStream<St>,
    f: F,
}

pub(crate) struct InspectErrProj<'pin, St: 'pin, F: 'pin> {
    pub stream: Pin<&'pin mut IntoStream<St>>,
    pub f: &'pin mut F,
}

impl<St, F> InspectErr<St, F>
where
    St: TryStream,
    F: FnMut(&St::Error),
{
    /// Construct a new `InspectErr` wrapper.
    pub fn new(stream: St, f: F) -> Self {
        let stream = IntoStream::new(stream);
        Self { stream, f }
    }

    /// Return a mutable reference to the underlying original stream `St`.
    pub fn get_mut(&mut self) -> &mut St {
        self.stream.get_mut()
    }

    pub(crate) fn project<'pin>(self: Pin<&'pin mut Self>) -> InspectErrProj<'pin, St, F> {
        unsafe {
            let this = self.get_unchecked_mut();
            InspectErrProj {
                stream: Pin::new_unchecked(&mut this.stream),
                f: &mut this.f,
            }
        }
    }

    /// Return a pinned mutable reference to the underlying original stream `St`.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        self.project().stream.get_pin_mut()
    }

    /// Consume wrapper and return the underlying original stream `St`.
    pub fn into_inner(self) -> St {
        self.stream.into_inner()
    }
}

//
// Debug impls (delegate to inner types' Debug where available)
//
impl<St, F> fmt::Debug for InspectErr<St, F>
where
    F: fmt::Debug,
    St: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.stream, f)?;
        fmt::Debug::fmt(&self.f, f)
    }
}

//
// Stream + FusedStream delegations
//
impl<St, F> tokio_stream::Stream for InspectErr<St, F>
where
    St: TryStream,
    F: FnMut(&<St as crate::try_stream::TryStream>::Error),
{
    type Item = Result<St::Ok, St::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let proj = self.project();
        match proj.stream.poll_next(cx) {
            Poll::Ready(Some(Ok(ok))) => Poll::Ready(Some(Ok(ok))),
            Poll::Ready(Some(Err(err))) => {
                (proj.f)(&err);
                Poll::Ready(Some(Err(err)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

impl<St, F> FusedStream for InspectErr<St, F>
where
    St: TryStream,
    F: FnMut(&St::Error),
    IntoStream<St>: FusedStream + tokio_stream::Stream,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

#[cfg(feature = "sink")]
use async_sink::Sink;
#[cfg(feature = "sink")]
impl<St, Item, F> Sink<Item> for InspectErr<St, F>
where
    St: TryStream + Sink<Item>,
    F: FnMut(&<St as crate::try_stream::TryStream>::Error),
{
    type Error = <St as async_sink::Sink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = unsafe { self.get_unchecked_mut() };
        let into_stream = unsafe { Pin::new_unchecked(&mut this.stream) };
        into_stream.get_pin_mut().poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let this = unsafe { self.get_unchecked_mut() };
        let into_stream = unsafe { Pin::new_unchecked(&mut this.stream) };
        into_stream.get_pin_mut().start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = unsafe { self.get_unchecked_mut() };
        let into_stream = unsafe { Pin::new_unchecked(&mut this.stream) };
        into_stream.get_pin_mut().poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = unsafe { self.get_unchecked_mut() };
        let into_stream = unsafe { Pin::new_unchecked(&mut this.stream) };
        into_stream.get_pin_mut().poll_close(cx)
    }
}
