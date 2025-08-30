use core::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use super::Inspect;
use crate::{
    fns::{inspect_err_fn, InspectErrFn},
    try_stream::IntoStream,
    FusedStream,
};

/// Stream for the [`inspect_err`](super::TryStreamExt::inspect_err) method.
pub struct InspectErr<St, F>(Inspect<IntoStream<St>, InspectErrFn<F>>);

impl<St, F> InspectErr<St, F> {
    pub fn new(stream: St, f: F) -> Self {
        InspectErr(Inspect::new(IntoStream::new(stream), inspect_err_fn(f)))
    }

    pub fn get_mut(&mut self) -> &mut St {
        self.0.get_mut().get_mut()
    }

    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        let into_pin = inner.get_pin_mut();
        into_pin.get_pin_mut()
    }

    pub fn into_inner(self) -> St {
        self.0.into_inner().into_inner()
    }
}

impl<St, F> fmt::Debug for InspectErr<St, F>
where
    Inspect<IntoStream<St>, InspectErrFn<F>>: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<St, F> futures_core::stream::Stream for InspectErr<St, F>
where
    Inspect<IntoStream<St>, InspectErrFn<F>>: futures_core::stream::Stream,
{
    type Item = <Inspect<IntoStream<St>, InspectErrFn<F>> as futures_core::stream::Stream>::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        inner.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<St, F> FusedStream for InspectErr<St, F>
where
    Inspect<IntoStream<St>, InspectErrFn<F>>: FusedStream,
{
    fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }
}

#[cfg(feature = "sink")]
impl<St, Item, F> Sink<Item> for InspectErr<St, F>
where
    Inspect<IntoStream<St>, InspectErrFn<F>>: Sink<Item>,
{
    type Error = <Inspect<IntoStream<St>, InspectErrFn<F>> as Sink<Item>>::Error;

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
