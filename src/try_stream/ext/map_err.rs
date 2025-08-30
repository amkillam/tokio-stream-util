use core::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use tokio_stream::{adapters::Map, StreamExt};

use crate::{
    fns::{map_err_fn, MapErrFn},
    try_stream::IntoStream,
    FusedStream, TryStream,
};

/// Stream for the [`map_err`](super::TryStreamExt::map_err) method.
pub struct MapErr<St, F>(Map<IntoStream<St>, MapErrFn<F>>);

impl<St, F> MapErr<St, F>
where
    St: TryStream + Unpin,
    F: FnMut(St::Error) -> St::Error + FnOnce(St::Error) -> St::Error,
    MapErrFn<F>: Unpin
        + FnMut(Result<St::Ok, St::Error>) -> Result<St::Ok, St::Error>
        + FnOnce(Result<St::Ok, St::Error>) -> Result<St::Ok, St::Error>,
{
    /// Creates a new `MapErr` combinator.
    pub fn new(stream: St, f: F) -> Self {
        let stream = IntoStream::new(stream);
        MapErr(stream.map(map_err_fn(f)))
    }
}

impl<St, F> fmt::Debug for MapErr<St, F>
where
    Map<IntoStream<St>, MapErrFn<F>>: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<St, F> futures_core::stream::Stream for MapErr<St, F>
where
    Map<IntoStream<St>, MapErrFn<F>>: futures_core::stream::Stream,
{
    type Item = <Map<IntoStream<St>, MapErrFn<F>> as futures_core::stream::Stream>::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        inner.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<St, F> FusedStream for MapErr<St, F>
where
    Map<IntoStream<St>, MapErrFn<F>>: FusedStream,
{
    fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }
}

#[cfg(feature = "sink")]
use tokio_sink::Sink;
#[cfg(feature = "sink")]
impl<St, Item, F> Sink<Item> for MapErr<St, F>
where
    Map<IntoStream<St>, MapErrFn<F>>: Sink<Item>,
{
    type Error = <Map<IntoStream<St>, MapErrFn<F>> as Sink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let this = unsafe { Pin::get_unchecked_mut(self) };
        this.0.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        inner.poll_close(cx)
    }
}
