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
    /// Creates a new `InspectErr` combinator.
    ///
    /// This function is generally called by the [`inspect_err`](super::TryStreamExt::inspect_err) method.
    /// See the documentation of that method for more details.
    ///
    /// # Examples
    /// ```
    /// use tokio_stream::wrappers::IntervalStream;
    /// use tokio_stream::StreamExt;
    /// use tokio_stream::TryStreamExt;
    /// use std::time::Duration;
    /// use std::io;
    /// use std::pin::Pin;
    /// use std::task::{Context, Poll};
    /// use futures_core::stream::Stream;
    /// use futures_util::stream;
    /// use std::fmt;
    /// use std::error::Error;
    /// use std::result::Result;
    /// use tokio::time::interval;
    /// use tokio::runtime::Runtime;
    ///
    /// // A simple stream that produces an error
    /// struct ErrorStream {
    ///     count: usize,
    /// }
    ///
    /// impl Stream for ErrorStream {
    ///     type Item = Result<usize, io::Error>;
    ///
    ///     fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    ///         if self.count < 3 {
    ///             self.count += 1;
    ///             Poll::Ready(Some(Ok(self.count)))
    ///         } else if self.count == 3 {
    ///             self.count += 1;
    ///             Poll::Ready(Some(Err(io::Error::new(io::ErrorKind::Other, "an error occurred"))))
    ///         } else {
    ///             Poll::Ready(None)
    ///         }
    ///     }
    /// }
    ///
    /// impl fmt::Debug for ErrorStream {
    ///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    ///         f.debug_struct("ErrorStream")
    ///          .field("count", &self.count)
    ///          .finish()
    ///     }
    /// }
    ///
    /// let rt = Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     let error_stream = ErrorStream { count: 0 };
    ///     let inspected_stream: tokio_stream_util::InspectErr<ErrorStream, _> = error_stream.inspect_err(|e| {
    ///         eprintln!("Error encountered: {}", e);
    ///     });
    ///
    ///     let results: Vec<_> = inspected_stream.collect().await;
    ///     println!("Stream results: {:?}", results);
    /// });
    /// ```
    ///
    /// # Panics
    /// Panics if `f` panics.
    ///
    pub fn new(stream: St, f: F) -> Self {
        InspectErr(Inspect::new(IntoStream::new(stream), inspect_err_fn(f)))
    }

    /// Gets a mutable reference to the underlying stream.
    pub fn get_mut(&mut self) -> &mut St {
        self.0.get_mut().get_mut()
    }

    /// Gets a pinned mutable reference to the underlying stream.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        let inner = unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).0) };
        let into_pin = inner.get_pin_mut();
        into_pin.get_pin_mut()
    }

    /// Consumes this combinator, returning the underlying stream.
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
use tokio_sink::Sink;
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
