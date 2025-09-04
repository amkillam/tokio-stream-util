use core::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{try_stream::IntoStream, FusedStream, TryStream};

/// Stream for the [`map_ok`](super::TryStreamExt::map_ok) method.
pub struct MapOk<St, V, F> {
    stream: IntoStream<St>,
    f: F,
    _val: core::marker::PhantomData<V>,
}

pub(crate) struct MapOkProj<'pin, St: 'pin, V: 'pin, F: 'pin> {
    pub stream: Pin<&'pin mut IntoStream<St>>,
    pub f: &'pin mut F,
    _val: core::marker::PhantomData<V>,
}

impl<St, V, F> MapOk<St, V, F>
where
    St: TryStream + Unpin,
    F: FnMut(St::Ok) -> V,
{
    /// Creates a new `MapOk` combinator.
    pub fn new(try_stream: St, f: F) -> Self {
        let stream = IntoStream::new(try_stream);

        Self {
            stream,
            f,
            _val: core::marker::PhantomData,
        }
    }

    pub(crate) fn project<'pin>(self: Pin<&'pin mut Self>) -> MapOkProj<'pin, St, V, F> {
        unsafe {
            let this = self.get_unchecked_mut();
            MapOkProj {
                stream: Pin::new_unchecked(&mut this.stream),
                f: &mut this.f,
                _val: core::marker::PhantomData,
            }
        }
    }
}

impl<St, V, F> fmt::Debug for MapOk<St, V, F>
where
    St: fmt::Debug,
    F: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.stream, f)?;
        fmt::Debug::fmt(&self._val, f)?;
        fmt::Debug::fmt(&self.f, f)
    }
}

impl<St, V, F> tokio_stream::Stream for MapOk<St, V, F>
where
    St: TryStream + Unpin,
    F: FnMut(St::Ok) -> V,
{
    type Item = Result<V, St::Error>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut proj = self.project();

        match proj.stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(ok))) => Poll::Ready(Some(Ok((proj.f)(ok)))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

impl<St, V, F> FusedStream for MapOk<St, V, F>
where
    St: TryStream + Unpin,
    F: FnMut(St::Ok) -> V,
    IntoStream<St>: FusedStream,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

#[cfg(feature = "sink")]
use async_sink::Sink;
#[cfg(feature = "sink")]
impl<St, V, Item, F> Sink<Item> for MapOk<St, V, F>
where
    St: Sink<Item> + TryStream + Unpin,
    F: FnMut(<St as crate::try_stream::TryStream>::Ok) -> V,
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
