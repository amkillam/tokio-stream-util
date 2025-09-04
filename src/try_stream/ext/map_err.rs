use core::{
    error::Error,
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{try_stream::IntoStream, FusedStream, TryStream};

/// Stream for the [`map_err`](super::TryStreamExt::map_err) method.
pub struct MapErr<St, E, F> {
    stream: IntoStream<St>,
    _err: core::marker::PhantomData<E>,
    f: F,
}

pub(crate) struct MapErrProj<'pin, St: 'pin, F: 'pin> {
    pub stream: Pin<&'pin mut IntoStream<St>>,
    pub f: &'pin mut F,
}

impl<St, E, F> MapErr<St, E, F>
where
    St: TryStream,
    E: Error,
    F: FnMut(St::Error) -> E,
{
    /// Creates a new `MapErr` combinator.
    pub fn new(stream: St, f: F) -> Self {
        let stream = IntoStream::new(stream);
        Self {
            stream,
            f,
            _err: core::marker::PhantomData,
        }
    }

    pub(crate) fn project<'pin>(self: Pin<&'pin mut Self>) -> MapErrProj<'pin, St, F> {
        unsafe {
            let this = self.get_unchecked_mut();
            MapErrProj {
                stream: Pin::new_unchecked(&mut this.stream),
                f: &mut this.f,
            }
        }
    }
}

impl<St, E, F> fmt::Debug for MapErr<St, E, F>
where
    E: fmt::Debug,
    St: fmt::Debug,
    F: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.stream, f)?;
        fmt::Debug::fmt(&self._err, f)?;
        fmt::Debug::fmt(&self.f, f)
    }
}

impl<St, E, F> tokio_stream::Stream for MapErr<St, E, F>
where
    E: Error,
    St: TryStream + Unpin,
    F: FnMut(St::Error) -> E,
{
    type Item = Result<St::Ok, F::Output>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut proj = self.project();
        match proj.stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(ok))) => Poll::Ready(Some(Ok(ok))),
            Poll::Ready(Some(Err(err))) => {
                let new_err = (proj.f)(err);
                Poll::Ready(Some(Err(new_err)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

impl<St, E, F> FusedStream for MapErr<St, E, F>
where
    St: TryStream + Unpin,
    E: Error,
    F: FnMut(St::Error) -> E,
    IntoStream<St>: FusedStream,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

#[cfg(feature = "sink")]
use async_sink::Sink;
#[cfg(feature = "sink")]
impl<St, E, Item, F> Sink<Item> for MapErr<St, E, F>
where
    St: Sink<Item> + TryStream + Unpin,
    <St as crate::try_stream::TryStream>::Error: Error,
    E: Error,
    F: FnMut(<St as crate::try_stream::TryStream>::Error) -> E,
{
    type Error = <St as async_sink::Sink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut proj = self.project();
        proj.stream.as_mut().get_pin_mut().poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let mut proj = self.project();
        proj.stream.as_mut().get_pin_mut().start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut proj = self.project();
        proj.stream.as_mut().get_pin_mut().poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut proj = self.project();
        proj.stream.as_mut().get_pin_mut().poll_close(cx)
    }
}
