use core::fmt;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};

use either::Either;
use futures_core::TryFuture;
#[cfg(feature = "sink")]
use tokio_sink::Sink;
use tokio_stream::Stream;

use super::IntoStream;
use super::{FusedStream, TryStream};
use crate::flatten_unordered::{FlattenUnorderedWithFlowController, FlowController, FlowStep};
use crate::try_stream::IntoFuseStream;
use crate::TryStreamExt;

/// Stream for the [`try_flatten_unordered`](super::TryStreamExt::try_flatten_unordered) method.
#[must_use = "streams do nothing unless polled"]
pub struct TryFlattenUnordered<St>
where
    NestedTryStreamIntoEitherTryStream<St>: Stream,
{
    inner: FlattenUnorderedWithFlowController<
        NestedTryStreamIntoEitherTryStream<St>,
        PropagateBaseStreamError<St>,
    >,
}

struct TryFlattenUnorderedProj<'pin, St>
where
    NestedTryStreamIntoEitherTryStream<St>: Stream,
{
    inner: Pin<
        &'pin mut FlattenUnorderedWithFlowController<
            NestedTryStreamIntoEitherTryStream<St>,
            PropagateBaseStreamError<St>,
        >,
    >,
}

impl<St> Unpin for TryFlattenUnordered<St> where NestedTryStreamIntoEitherTryStream<St>: Stream {}

impl<St> fmt::Debug for TryFlattenUnordered<St>
where
    St: fmt::Debug,
    <NestedTryStreamIntoEitherTryStream<St> as Stream>::Item: Stream + fmt::Debug,
    NestedTryStreamIntoEitherTryStream<St>: Stream + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryFlattenUnordered")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<St> TryFlattenUnordered<St>
where
    St: TryStream,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
    FlattenUnorderedWithFlowController<
        NestedTryStreamIntoEitherTryStream<St>,
        PropagateBaseStreamError<St>,
    >: Stream<Item = Result<<St::Ok as TryStream>::Ok, <St::Ok as TryStream>::Error>>,
    Either<IntoStream<St::Ok>, SingleStreamResult<St::Ok>>:
        Stream<Item = Result<<St::Ok as TryStream>::Ok, <St::Ok as TryStream>::Error>>,
{
    pub(super) fn new(stream: St, limit: impl Into<Option<usize>>) -> Self {
        Self {
            inner: FlattenUnorderedWithFlowController::new(
                NestedTryStreamIntoEitherTryStream::new(stream),
                limit.into(),
            ),
        }
    }

    pub(crate) fn project<'pin>(self: Pin<&'pin mut Self>) -> TryFlattenUnorderedProj<'pin, St> {
        unsafe {
            let this = self.get_unchecked_mut();
            TryFlattenUnorderedProj {
                inner: Pin::new_unchecked(&mut this.inner),
            }
        }
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &St {
        self.inner.get_ref().get_ref()
    }

    /// Acquires a mutable reference to the underlying stream that this
    /// combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut St {
        self.inner.get_mut().get_mut()
    }

    /// Acquires a pinned mutable reference to the underlying stream that this
    /// combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        self.project().inner.get_pin_mut().get_pin_mut()
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> St {
        self.inner.into_inner().into_inner()
    }
}

impl<St> Stream for TryFlattenUnordered<St>
where
    St: TryStream,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
    FlattenUnorderedWithFlowController<
        NestedTryStreamIntoEitherTryStream<St>,
        PropagateBaseStreamError<St>,
    >: Stream<Item = Result<<St::Ok as TryStream>::Ok, <St::Ok as TryStream>::Error>>,
{
    type Item = Result<<St::Ok as TryStream>::Ok, <St::Ok as TryStream>::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().inner) }.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<St> FusedStream for TryFlattenUnordered<St>
where
    St: TryStream,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
    FlattenUnorderedWithFlowController<
        NestedTryStreamIntoEitherTryStream<St>,
        PropagateBaseStreamError<St>,
    >: Stream<Item = Result<<St::Ok as TryStream>::Ok, <St::Ok as TryStream>::Error>>,
    FlattenUnorderedWithFlowController<
        NestedTryStreamIntoEitherTryStream<St>,
        PropagateBaseStreamError<St>,
    >: FusedStream,
{
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}

#[cfg(feature = "sink")]
impl<St, Item> Sink<Item> for TryFlattenUnordered<St>
where
    St: TryStream + Sink<Item>,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
    <St as Sink<Item>>::Error: From<<St as TryStream>::Error>,
{
    type Error = <St as Sink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().inner) }.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().inner) }.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().inner) }.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().inner) }.poll_close(cx)
    }
}

/// Emits either successful streams or single-item streams containing the underlying errors.
/// This's a wrapper for `FlattenUnordered` to reuse its logic over `TryStream`.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct NestedTryStreamIntoEitherTryStream<St> {
    stream: IntoFuseStream<St>,
}

struct NestedTryStreamIntoEitherTryStreamProj<'pin, St>
where
    St: TryStream,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    stream: Pin<&'pin mut IntoFuseStream<St>>,
}

impl<St> Unpin for NestedTryStreamIntoEitherTryStream<St>
where
    St: TryStream + Unpin,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
}

impl<St> NestedTryStreamIntoEitherTryStream<St>
where
    St: TryStream,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    fn new(stream: St) -> Self {
        Self {
            stream: IntoFuseStream::new(stream),
        }
    }

    pub(crate) fn project<'pin>(
        self: Pin<&'pin mut Self>,
    ) -> NestedTryStreamIntoEitherTryStreamProj<'pin, St> {
        unsafe {
            let this = self.get_unchecked_mut();
            NestedTryStreamIntoEitherTryStreamProj {
                stream: Pin::new_unchecked(&mut this.stream),
            }
        }
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &St {
        self.stream.get_ref()
    }

    /// Acquires a mutable reference to the underlying stream that this
    /// combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut St {
        self.stream.get_mut()
    }

    /// Acquires a pinned mutable reference to the underlying stream that this
    /// combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        self.project().stream.get_pin_mut()
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> St {
        self.stream.into_inner()
    }
}

/// Emits a single item immediately, then stream will be terminated.
#[derive(Debug, Clone)]
pub struct Single<T>(Option<T>);

impl<T> Single<T> {
    /// Constructs new `Single` with the given value.
    fn new(val: T) -> Self {
        Self(Some(val))
    }

    /// Attempts to take inner item immediately. Will always succeed if the stream isn't terminated.
    fn next_immediate(&mut self) -> Option<T> {
        self.0.take()
    }
}

impl<T> Unpin for Single<T> {}

impl<T> Stream for Single<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.take())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.as_ref().map_or((0, Some(0)), |_| (1, Some(1)))
    }
}

/// Immediately propagates errors occurred in the base stream.
#[derive(Debug, Clone, Copy)]
pub struct PropagateBaseStreamError<St>(PhantomData<St>);

type BaseStreamItem<St> = <NestedTryStreamIntoEitherTryStream<St> as Stream>::Item;
type InnerStreamItem<St> = <BaseStreamItem<St> as Stream>::Item;

impl<St> FlowController<BaseStreamItem<St>, InnerStreamItem<St>> for PropagateBaseStreamError<St>
where
    St: TryStream,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
    Either<IntoStream<St::Ok>, SingleStreamResult<St::Ok>>:
        Stream<Item = Result<<St::Ok as TryStream>::Ok, <St::Ok as TryStream>::Error>>,
{
    fn next_step(item: BaseStreamItem<St>) -> FlowStep<BaseStreamItem<St>, InnerStreamItem<St>> {
        match item {
            // A new successful inner stream received
            st @ Either::Left(_) => FlowStep::Continue(st),
            // An error encountered
            Either::Right(mut err) => FlowStep::Return(err.next_immediate().unwrap()),
        }
    }
}

pub(crate) type SingleStreamResult<St> =
    Single<Result<<St as TryStream>::Ok, <St as TryStream>::Error>>;

impl<St> Stream for NestedTryStreamIntoEitherTryStream<St>
where
    St: TryStream,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    // Item is either an inner stream or a stream containing a single error.
    // This will allow using `Either`'s `Stream` implementation as both branches are actually streams of `Result`'s.
    type Item = Either<IntoStream<St::Ok>, SingleStreamResult<St::Ok>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let stream = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) };
        let item = match stream.try_poll_next(cx) {
            Poll::Ready(x) => x,
            Poll::Pending => return Poll::Pending,
        };

        let out = match item {
            Some(res) => match res {
                // Emit successful inner stream as is
                Ok(stream) => Either::Left(stream.into_stream()),
                // Wrap an error into a stream containing a single item
                err @ Err(_) => {
                    let res = err.map(|_: St::Ok| unreachable!()).map_err(Into::into);

                    Either::Right(Single::new(res))
                }
            },
            None => return Poll::Ready(None),
        };

        Poll::Ready(Some(out))
    }
}

impl<St> FusedStream for NestedTryStreamIntoEitherTryStream<St>
where
    NestedTryStreamIntoEitherTryStream<St>: Stream,
    St: TryStream,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

// Forwarding impl of Sink from the underlying stream
#[cfg(feature = "sink")]
impl<St, Item> Sink<Item> for NestedTryStreamIntoEitherTryStream<St>
where
    St: TryStream + Sink<Item>,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<<St as TryStream>::Error>,
{
    type Error = <St as Sink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) }.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) }.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) }.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) }.poll_close(cx)
    }
}
