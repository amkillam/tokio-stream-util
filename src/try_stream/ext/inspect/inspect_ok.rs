use super::Inspect;
use crate::fns::{inspect_ok_fn, InspectOkFn};
use crate::try_stream::IntoStream;

use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};

use futures_core::stream::FusedStream;

/// Stream for the [`inspect_ok`](super::TryStreamExt::inspect_ok) method.
pub struct InspectOk<St, F>(Inspect<IntoStream<St>, InspectOkFn<F>>);

impl<St, F> InspectOk<St, F> {
    /// Construct a new `InspectOk` wrapper.
    pub fn new(stream: St, f: F) -> Self {
        InspectOk(Inspect::new(IntoStream::new(stream), inspect_ok_fn(f)))
    }

    /// Return a mutable reference to the underlying original stream `St`.
    pub fn get_mut(&mut self) -> &mut St {
        // Map/Inspect public API provide get_mut that returns &mut inner stream.
        // First get mut reference to the inner `Inspect<IntoStream<St>, _>`,
        // then to the `IntoStream<St>`, then to `St`.
        self.0.get_mut().get_mut()
    }

    /// Return a pinned mutable reference to the underlying original stream `St`.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        // Project to Pin<&mut Map/Inspect>
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        // Map/Inspect have `get_pin_mut` returning `Pin<&mut IntoStream<St>>`
        let into_pin = inner.get_pin_mut();
        // IntoStream::get_pin_mut returns Pin<&mut St>
        into_pin.get_pin_mut()
    }

    /// Consume wrapper and return the underlying original stream `St`.
    pub fn into_inner(self) -> St {
        self.0.into_inner().into_inner()
    }
}

//
// Debug impls (delegate to inner types' Debug where available)
//

impl<St, F> fmt::Debug for InspectOk<St, F>
where
    Inspect<IntoStream<St>, InspectOkFn<F>>: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

//
// Stream + FusedStream delegations
//

impl<St, F> futures_core::stream::Stream for InspectOk<St, F>
where
    Inspect<IntoStream<St>, InspectOkFn<F>>: futures_core::stream::Stream,
{
    type Item = <Inspect<IntoStream<St>, InspectOkFn<F>> as futures_core::stream::Stream>::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Delegate to inner `Inspect<IntoStream<St>, _>`
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        inner.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<St, F> FusedStream for InspectOk<St, F>
where
    Inspect<IntoStream<St>, InspectOkFn<F>>: FusedStream,
{
    fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }
}

#[cfg(feature = "sink")]
use tokio_sink::Sink;
#[cfg(feature = "sink")]
//
// Optional Sink forwarding when feature = "sink"
//
impl<St, Item, F> Sink<Item> for InspectOk<St, F>
where
    Inspect<IntoStream<St>, InspectOkFn<F>>: Sink<Item>,
{
    type Error = <Inspect<IntoStream<St>, InspectOkFn<F>> as Sink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        // `start_send` does not require pin.
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
