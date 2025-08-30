use core::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use tokio_stream::{adapters::Map, StreamExt};

use crate::{
    fns::{map_ok_fn, MapOkFn},
    try_stream::IntoStream,
    FusedStream, TryStream,
};

/// Stream for the [`map_ok`](super::TryStreamExt::map_ok) method.
pub struct MapOk<St, F>(Map<IntoStream<St>, MapOkFn<F>>);

impl<St, F> MapOk<St, F>
where
    St: TryStream + Unpin,
    F: FnMut(St::Ok) -> St::Ok + FnOnce(St::Ok) -> St::Ok,
    MapOkFn<F>: Unpin
        + FnMut(Result<St::Ok, St::Error>) -> Result<St::Ok, St::Error>
        + FnOnce(Result<St::Ok, St::Error>) -> Result<St::Ok, St::Error>,
{
    pub fn new(try_stream: St, f: F) -> Self {
        let stream = IntoStream::new(try_stream);
        MapOk(stream.map(map_ok_fn(f)))
    }
}

impl<St, F> fmt::Debug for MapOk<St, F>
where
    Map<IntoStream<St>, MapOkFn<F>>: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<St, F> futures_core::stream::Stream for MapOk<St, F>
where
    Map<IntoStream<St>, MapOkFn<F>>: futures_core::stream::Stream,
{
    type Item = <Map<IntoStream<St>, MapOkFn<F>> as futures_core::stream::Stream>::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        inner.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<St, F> FusedStream for MapOk<St, F>
where
    Map<IntoStream<St>, MapOkFn<F>>: FusedStream,
{
    fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }
}

#[cfg(feature = "sink")]
impl<St, Item, F> Sink<Item> for MapOk<St, F>
where
    Map<IntoStream<St>, MapOkFn<F>>: Sink<Item>,
{
    type Error = <Map<IntoStream<St>, MapOkFn<F>> as Sink<Item>>::Error;

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
