use core::fmt;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::ready;

use either::Either;
use tokio_stream::Stream;

use super::IntoStream;
use super::{FusedStream, TryStream};
use crate::flatten_unordered::{FlattenUnorderedWithFlowController, FlowController, FlowStep};
use crate::{try_stream::IntoFuseStream, TryStreamExt};

#[derive(Debug)]
/// An enum to implement `Stream` for `either::Either`.
pub enum EitherStream<L, R> {
    Left(L),
    Right(R),
}

impl<L: Unpin, R: Unpin> Unpin for EitherStream<L, R> {}

impl<L, R> From<Either<L, R>> for EitherStream<L, R> {
    fn from(e: Either<L, R>) -> Self {
        match e {
            Either::Left(l) => Self::Left(l),
            Either::Right(r) => Self::Right(r),
        }
    }
}

impl<L, R> Stream for EitherStream<L, R>
where
    L: Stream + Unpin,
    R: Stream<Item = L::Item> + Unpin,
{
    type Item = L::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        unsafe {
            match self.get_unchecked_mut() {
                EitherStream::Left(l) => Pin::new_unchecked(l).poll_next(cx),
                EitherStream::Right(r) => Pin::new_unchecked(r).poll_next(cx),
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            EitherStream::Left(l) => l.size_hint(),
            EitherStream::Right(r) => r.size_hint(),
        }
    }
}

impl<L, R> FusedStream for EitherStream<L, R>
where
    L: FusedStream + Unpin,
    R: FusedStream<Item = L::Item> + Unpin,
{
    fn is_terminated(&self) -> bool {
        match self {
            EitherStream::Left(l) => l.is_terminated(),
            EitherStream::Right(r) => r.is_terminated(),
        }
    }
}

/// Stream for the [`try_flatten_unordered`](super::TryStreamExt::try_flatten_unordered) method.
#[must_use = "streams do nothing unless polled"]
pub struct TryFlattenUnordered<St: TryStream>
where
    St::Ok: TryStream + Unpin,
    <St as TryStream>::Ok: Stream,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    inner: FlattenUnorderedWithFlowController<
        NestedTryStreamIntoEither<St>,
        PropagateBaseStreamError<St>,
    >,
}

pub(crate) struct TryFlattenUnorderedProj<'pin, St: TryStream>
where
    St::Ok: TryStream + Unpin,
    <St as TryStream>::Ok: Stream,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    inner: Pin<
        &'pin mut FlattenUnorderedWithFlowController<
            NestedTryStreamIntoEither<St>,
            PropagateBaseStreamError<St>,
        >,
    >,
}

impl<St: TryStream> Unpin for TryFlattenUnordered<St>
where
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
}

impl<St: TryStream> fmt::Debug for TryFlattenUnordered<St>
where
    St: fmt::Debug,
    St::Ok: TryStream + Unpin + fmt::Debug,
    <St::Ok as TryStream>::Ok: fmt::Debug,
    <St::Ok as TryStream>::Error: From<St::Error> + fmt::Debug,
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
{
    pub(super) fn new(stream: St, limit: Option<usize>) -> Self {
        Self {
            inner: FlattenUnorderedWithFlowController::new(
                NestedTryStreamIntoEither::new(stream),
                limit,
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
    <St::Ok as TryStream>::Error: From<<St as TryStream>::Error>,
    <St as Sink<Item>>::Error: From<<St as TryStream>::Error>,
{
    type Error = <St as Sink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().inner) };
        inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let inner = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().inner) };
        inner.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().inner) };
        inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().inner) };
        inner.poll_close(cx)
    }
}

/// Emits either successful streams or single-item streams containing the underlying errors.
/// This's a wrapper for `FlattenUnordered` to reuse its logic over `TryStream`.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct NestedTryStreamIntoEither<St> {
    stream: IntoFuseStream<St>,
}

pub(crate) struct NestedTryStreamIntoEitherProj<'pin, St> {
    stream: Pin<&'pin mut IntoFuseStream<St>>,
}

impl<St> NestedTryStreamIntoEither<St>
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
    ) -> NestedTryStreamIntoEitherProj<'pin, St> {
        unsafe {
            let this = self.get_unchecked_mut();
            NestedTryStreamIntoEitherProj {
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

type BaseStreamItem<St> = <NestedTryStreamIntoEither<St> as Stream>::Item;
type InnerStreamItem<St> = <BaseStreamItem<St> as Stream>::Item;

impl<St> FlowController<BaseStreamItem<St>, InnerStreamItem<St>> for PropagateBaseStreamError<St>
where
    St: TryStream,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    fn next_step(item: BaseStreamItem<St>) -> FlowStep<BaseStreamItem<St>, InnerStreamItem<St>> {
        match item {
            // A new successful inner stream received
            st @ EitherStream::Left(_) => FlowStep::Continue(st),
            // An error encountered
            EitherStream::Right(mut err) => FlowStep::Return(err.next_immediate().unwrap()),
        }
    }
}

pub(crate) type SingleStreamResult<Ok, Error> = Single<Result<Ok, Error>>;

impl<St> Stream for NestedTryStreamIntoEither<St>
where
    St: TryStream,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    // Item is either an inner stream or a stream containing a single error.
    // This will allow using `Either`'s `Stream` implementation as both branches are actually streams of
    // `Result`'s.
    type Item = EitherStream<
        IntoStream<St::Ok>,
        SingleStreamResult<<St::Ok as TryStream>::Ok, <St::Ok as TryStream>::Error>,
    >;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let stream = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) };
        let item = ready!(stream.try_poll_next(cx));

        Poll::Ready(item.map(|res| {
            match res {
                // Emit successful inner stream as is
                Ok(stream) => Either::Left(stream.into_stream()),
                // Wrap an error into a stream containing a single item
                Err(e) => {
                    let err: Result<<St::Ok as TryStream>::Ok, _> = Err(e.into());
                    Either::Right(Single::new(err))
                }
            }
            .into()
        }))
    }
}

impl<St> FusedStream for NestedTryStreamIntoEither<St>
where
    St: TryStream,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<St::Error>,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

#[cfg(feature = "sink")]
use tokio_sink::Sink;
#[cfg(feature = "sink")]
// Forwarding impl of Sink from the underlying stream
impl<St, Item> Sink<Item> for NestedTryStreamIntoEither<St>
where
    St: TryStream + Sink<Item>,
    St::Ok: TryStream + Unpin,
    <St::Ok as TryStream>::Error: From<<St as TryStream>::Error>,
{
    type Error = <St as Sink<Item>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let stream = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) };
        stream.get_pin_mut().poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let stream = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) };
        stream.get_pin_mut().start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let stream = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) };
        stream.get_pin_mut().poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let stream = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().stream) };
        stream.get_pin_mut().poll_close(cx)
    }
}
