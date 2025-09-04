//! Streams
//!
//! This module contains a number of functions for working with `Streams`s
//! that return `Result`s, allowing for short-circuiting computations.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::TryFuture;

use super::TryStream;
use crate::FusedStream;

mod and_then;
pub use and_then::AndThen;

mod err_into;
pub use err_into::ErrInto;

mod inspect;
pub use inspect::InspectOk;

pub use inspect::InspectErr;

mod into_stream;

#[cfg(feature = "alloc")]
#[cfg_attr(target_os = "none", cfg(target_has_atomic = "ptr"))]
pub(crate) use into_stream::IntoFuseStream;

pub use into_stream::IntoStream;

mod map_ok;
pub use map_ok::MapOk;

mod map_err;
pub use map_err::MapErr;

mod or_else;
pub use or_else::OrElse;

mod try_next;
pub use try_next::TryNext;

mod try_filter;
pub use try_filter::TryFilter;

#[cfg(all(feature = "sink", feature = "alloc"))]
mod try_forward;
#[cfg(all(feature = "sink", feature = "alloc"))]
pub use try_forward::TryForward;

mod try_filter_map;
pub use try_filter_map::TryFilterMap;

mod try_flatten;
pub use try_flatten::TryFlatten;

#[cfg(all(feature = "alloc", feature = "std"))]
#[cfg_attr(target_os = "none", cfg(target_has_atomic = "ptr"))]
mod try_flatten_unordered;
#[cfg(all(feature = "alloc", feature = "std"))]
#[cfg_attr(target_os = "none", cfg(target_has_atomic = "ptr"))]
pub use try_flatten_unordered::TryFlattenUnordered;

mod try_collect;
pub use try_collect::TryCollect;

mod try_concat;
pub use try_concat::TryConcat;

#[cfg(feature = "alloc")]
mod try_chunks;
#[cfg(feature = "alloc")]
pub use try_chunks::{TryChunks, TryChunksError};

#[cfg(feature = "alloc")]
mod try_ready_chunks;
#[cfg(feature = "alloc")]
pub use try_ready_chunks::{TryReadyChunks, TryReadyChunksError};

mod try_unfold;
pub use try_unfold::{try_unfold, TryUnfold};

mod try_skip_while;
pub use try_skip_while::TrySkipWhile;

mod try_take_while;
pub use try_take_while::TryTakeWhile;

#[cfg(feature = "alloc")]
#[cfg_attr(target_os = "none", cfg(target_has_atomic = "ptr"))]
mod try_buffer_unordered;
#[cfg(feature = "alloc")]
#[cfg_attr(target_os = "none", cfg(target_has_atomic = "ptr"))]
pub use try_buffer_unordered::TryBufferUnordered;

#[cfg(feature = "alloc")]
#[cfg_attr(target_os = "none", cfg(target_has_atomic = "ptr"))]
mod try_buffered;
#[cfg(feature = "alloc")]
#[cfg_attr(target_os = "none", cfg(target_has_atomic = "ptr"))]
pub use try_buffered::TryBuffered;

#[cfg(all(feature = "io", feature = "std"))]
mod into_async_read;
#[cfg(all(feature = "io", feature = "std"))]
pub use into_async_read::IntoAsyncRead;

mod try_all;
pub use try_all::TryAll;

mod try_any;
pub use try_any::TryAny;

impl<S: ?Sized + TryStream> TryStreamExt for S {}

/// Adapters specific to `Result`-returning streams
pub trait TryStreamExt: TryStream {
    /// Wraps the current stream in a new stream which converts the error type
    /// into the one provided.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// #[tokio::main]
    /// async fn main() {
    /// let stream =
    ///     tokio_stream::iter(vec![Ok::<(), i32>(()), Err::<(), i32>(5)])
    ///         .err_into::<i64>();
    ///
    /// let collected =  stream.into_stream().collect::<Vec<_>>().await;
    /// assert_eq!(collected, vec![Ok(()), Err(5i64)]);
    /// }
    /// ```
    fn err_into<E>(self) -> ErrInto<Self, E>
    where
        Self: Sized,
        Self::Error: Into<E>,
    {
        ErrInto::new(self)
    }

    /// Wraps the current stream in a new stream which maps the success value
    /// using the provided closure.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// #[tokio::main]
    /// async fn main() {
    /// let stream =
    ///     tokio_stream::iter(vec![Ok::<i32, i32>(5), Err::<i32, i32>(0)])
    ///         .map_ok(|x| x + 2);
    ///
    /// let out =  stream.into_stream().collect::<Vec<_>>().await;
    /// assert_eq!(out, vec![Ok(7), Err(0)]);
    /// }
    /// ```
    fn map_ok<V, F>(self, f: F) -> MapOk<Self, V, F>
    where
        Self: TryStream + Unpin + Sized,
        F: FnMut(Self::Ok) -> V,
    {
        MapOk::new(self, f)
    }

    /// Wraps the current stream in a new stream which maps the error value
    /// using the provided closure.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// #[derive(Debug)]
    /// struct MyErr(String);
    /// impl core::fmt::Display for MyErr { fn fmt(&self, f:&mut core::fmt::Formatter<'_>)->core::fmt::Result{ self.0.fmt(f)} }
    /// impl core::error::Error for MyErr {}
    ///
    /// let stream =
    ///     tokio_stream::iter(vec![Ok::<i32, i32>(5), Err::<i32, i32>(0)])
    ///         .map_err(|x| MyErr(format!("e{}", x)));
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// let out = rt.block_on(async { stream.into_stream().collect::<Vec<_>>().await });
    /// assert_eq!(format!("{:?}", out), format!("{:?}", vec![Ok(5), Err(MyErr(String::from("e0")))]));
    /// ```
    fn map_err<E, F>(self, f: F) -> MapErr<Self, E, F>
    where
        Self: TryStream + Unpin + Sized,
        E: core::error::Error,
        F: FnMut(Self::Error) -> E,
    {
        MapErr::new(self, f)
    }

    /// Chain on a computation for when a value is ready, passing the successful
    /// results to the provided closure `f`.
    ///
    /// This function can be used to run a unit of work when the next successful
    /// value on a stream is ready. The closure provided will be yielded a value
    /// when ready, and the returned future will then be run to completion to
    /// produce the next value on this stream.
    ///
    /// Any errors produced by this stream will not be passed to the closure,
    /// and will be passed through.
    ///
    /// The returned value of the closure must implement the `TryFuture` trait
    /// and can represent some more work to be done before the composed stream
    /// is finished.
    ///
    /// Note that this function consumes the receiving stream and returns a
    /// wrapped version of it.
    ///
    /// To process the entire stream and return a single future representing
    /// success or error, use `try_for_each` instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let stream = tokio_stream::iter(vec![Ok::<i32, ()>(1), Ok(2)])
    ///         .and_then(|result| async move {
    ///             Ok(if result % 2 == 0 { Some(result) } else { None })
    ///         });
    ///
    ///     let out = stream.into_stream().collect::<Vec<_>>().await;
    ///     assert_eq!(out, vec![Ok(None), Ok(Some(2))]);
    /// });
    /// ```
    fn and_then<Fut, F>(self, f: F) -> AndThen<Self, Fut, F>
    where
        F: FnMut(Self::Ok) -> Fut,
        Fut: TryFuture<Error = Self::Error>,
        Self: Sized,
    {
        AndThen::new(self, f)
    }

    /// Chain on a computation for when an error happens, passing the
    /// erroneous result to the provided closure `f`.
    ///
    /// This function can be used to run a unit of work and attempt to recover from
    /// an error if one happens. The closure provided will be yielded an error
    /// when one appears, and the returned future will then be run to completion
    /// to produce the next value on this stream.
    ///
    /// Any successful values produced by this stream will not be passed to the
    /// closure, and will be passed through.
    ///
    /// The returned value of the closure must implement the [`TryFuture`](futures_core::future::TryFuture) trait
    /// and can represent some more work to be done before the composed stream
    /// is finished.
    ///
    /// Note that this function consumes the receiving stream and returns a
    /// wrapped version of it.
    fn or_else<Fut, F>(self, f: F) -> OrElse<Self, Fut, F>
    where
        F: FnMut(Self::Error) -> Fut,
        Fut: TryFuture<Ok = Self::Ok>,
        Self: Sized,
    {
        OrElse::new(self, f)
    }

    #[cfg(all(feature = "sink", feature = "alloc"))]
    /// A future that completes after the given stream has been fully processed
    /// into the sink and the sink has been flushed and closed.
    ///
    /// This future will drive the stream to keep producing items until it is
    /// exhausted, sending each item to the sink. It will complete once the
    /// stream is exhausted, the sink has received and flushed all items, and
    /// the sink is closed. Note that neither the original stream nor provided
    /// sink will be output by this future. Pass the sink by `Pin<&mut S>`
    /// (for example, via `try_forward(&mut sink)` inside an `async` fn/block) in
    /// order to preserve access to the `Sink`. If the stream produces an error,
    /// that error will be returned by this future without flushing/closing the sink.
    fn try_forward<S>(self, sink: S) -> TryForward<Self, S>
    where
        S: async_sink::Sink<Self::Ok, Error = Self::Error>,
        Self: Sized,
    {
        TryForward::new(self, sink)
    }

    /// Do something with the success value of this stream, afterwards passing
    /// it on.
    ///
    /// This is similar to the `StreamExt::inspect` method where it allows
    /// easily inspecting the success value as it passes through the stream, for
    /// example to debug what's going on.
    fn inspect_ok<F>(self, f: F) -> InspectOk<Self, F>
    where
        F: FnMut(&Self::Ok),
        Self: Sized,
    {
        InspectOk::new(self, f)
    }

    /// Do something with the error value of this stream, afterwards passing it on.
    ///
    /// This is similar to the `StreamExt::inspect` method where it allows
    /// easily inspecting the error value as it passes through the stream, for
    /// example to debug what's going on.
    fn inspect_err<F>(self, f: F) -> InspectErr<Self, F>
    where
        F: FnMut(&Self::Error),
        Self: Sized,
    {
        InspectErr::new(self, f)
    }

    /// Wraps a [`TryStream`] into a type that implements
    /// [`Stream`](tokio_stream::Stream)
    ///
    /// [`TryStream`]s currently do not implement the
    /// [`Stream`](tokio_stream::Stream) trait because of limitations
    /// of the compiler.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::{TryStream, TryStreamExt};
    ///
    /// type T = i32;
    /// type E = ();
    ///
    /// fn make_try_stream() -> impl TryStream<Ok = T, Error = E> {
    ///     tokio_stream::iter(vec![Ok::<T, E>(1), Ok(2), Err(())])
    /// }
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let out = make_try_stream().into_stream().collect::<Vec<_>>().await;
    ///     assert_eq!(out, vec![Ok(1), Ok(2), Err(())]);
    /// });
    /// ```
    fn into_stream(self) -> IntoStream<Self>
    where
        Self: Sized,
    {
        IntoStream::new(self)
    }

    /// Creates a future that attempts to resolve the next item in the stream.
    /// If an error is encountered before the next item, the error is returned
    /// instead.
    ///
    /// This is similar to the `Stream::next` combinator, but returns a
    /// `Result<Option<T>, E>` rather than an `Option<Result<T, E>>`, making
    /// for easy use with the `?` operator.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let mut stream = tokio_stream::iter(vec![Ok::<(), ()>(()), Err::<(), ()>(())]);
    ///     assert_eq!(stream.try_next().await, Ok(Some(())));
    /// });
    /// ```
    fn try_next(&mut self) -> TryNext<'_, Self>
    where
        Self: Unpin,
    {
        TryNext::new(self)
    }

    /// Skip elements on this stream while the provided asynchronous predicate
    /// resolves to `true`.
    ///
    /// This function is similar to
    /// [`StreamExt::skip_while`](futures_util::stream::StreamExt::skip_while) but exits
    /// early if an error occurs.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let stream = tokio_stream::iter(vec![Ok::<i32, i32>(1), Ok(3), Ok(2)])
    ///         .try_skip_while(|x| {
    ///             let v = *x;
    ///             async move { Ok(v < 3) }
    ///         });
    ///
    ///     let out = stream.into_stream().collect::<Vec<_>>().await;
    ///     assert_eq!(out, vec![Ok(3), Ok(2)]);
    /// });
    /// ```
    fn try_skip_while<Fut, F>(self, f: F) -> TrySkipWhile<Self, Fut, F>
    where
        F: FnMut(&Self::Ok) -> Fut,
        Fut: TryFuture<Ok = bool, Error = Self::Error>,
        Self: Sized,
    {
        TrySkipWhile::new(self, f)
    }

    /// Take elements on this stream while the provided asynchronous predicate
    /// resolves to `true`.
    ///
    /// This function is similar to
    /// [`StreamExt::take_while`](futures_util::stream::StreamExt::take_while) but exits
    /// early if an error occurs.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let stream = tokio_stream::iter(vec![Ok::<i32, i32>(1), Ok(2), Ok(3), Ok(2)])
    ///         .try_take_while(|x| {
    ///             let v = *x;
    ///             async move { Ok(v < 3) }
    ///         });
    ///
    ///     let out = stream.into_stream().collect::<Vec<_>>().await;
    ///     assert_eq!(out, vec![Ok(1), Ok(2)]);
    /// });
    /// ```
    fn try_take_while<Fut, F>(self, f: F) -> TryTakeWhile<Self, Fut, F>
    where
        F: FnMut(&Self::Ok) -> Fut,
        Fut: TryFuture<Ok = bool, Error = Self::Error>,
        Self: Sized,
    {
        TryTakeWhile::new(self, f)
    }

    /// Attempt to transform a stream into a collection,
    /// returning a future representing the result of that computation.
    ///
    /// This combinator will collect all successful results of this stream and
    /// collect them into the specified collection type. If an error happens then all
    /// collected elements will be dropped and the error will be returned.
    ///
    /// The returned future will be resolved when the stream terminates.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let future = tokio_stream::iter(vec![Ok::<i32, i32>(1), Ok(2), Err(3)])
    ///         .try_collect::<Vec<_>>();
    ///
    ///     assert_eq!(future.await, Err(3));
    /// });
    /// ```
    fn try_collect<C: Default + Extend<Self::Ok>>(self) -> TryCollect<Self, C>
    where
        Self: Sized,
    {
        TryCollect::new(self)
    }

    /// An adaptor for chunking up successful items of the stream inside a vector.
    ///
    /// This combinator will attempt to pull successful items from this stream and buffer
    /// them into a local vector. At most `capacity` items will get buffered
    /// before they're yielded from the returned stream.
    ///
    /// Note that the vectors returned from this iterator may not always have
    /// `capacity` elements. If the underlying stream ended and only a partial
    /// vector was created, it'll be returned. Additionally if an error happens
    /// from the underlying stream then the currently buffered items will be
    /// yielded.
    ///
    /// This method is only available when the `std` or `alloc` feature of this
    /// library is activated, and it is activated by default.
    ///
    /// This function is similar to
    /// [`StreamExt::chunks`](futures_util::stream::StreamExt::chunks) but exits
    /// early if an error occurs.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    /// use tokio_stream_util::try_stream::TryChunksError;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let stream = tokio_stream::iter(vec![
    ///         Ok::<i32, i32>(1), Ok(2), Ok(3), Err(4), Ok(5), Ok(6)
    ///     ]).try_chunks(2);
    ///
    ///     let out = stream.into_stream().collect::<Vec<_>>().await;
    ///     assert_eq!(out, vec![
    ///         Ok(vec![1, 2]),
    ///         Ok(vec![3]),
    ///         Err(TryChunksError::<i32, i32>(vec![], 4)),
    ///         Ok(vec![5, 6]),
    ///     ]);
    /// });
    /// ```
    ///
    /// # Panics
    ///
    /// This method will panic if `capacity` is zero.
    #[cfg(feature = "alloc")]
    fn try_chunks(self, capacity: usize) -> TryChunks<Self>
    where
        <IntoFuseStream<Self> as tokio_stream::Stream>::Item: core::fmt::Debug,
        Self: Sized,
    {
        TryChunks::new(self, capacity)
    }

    /// An adaptor for chunking up successful, ready items of the stream inside a vector.
    ///
    /// This combinator will attempt to pull successful items from this stream and buffer
    /// them into a local vector. At most `capacity` items will get buffered
    /// before they're yielded from the returned stream. If the underlying stream
    /// returns `Poll::Pending`, and the collected chunk is not empty, it will
    /// be immediately returned.
    ///
    /// Note that the vectors returned from this iterator may not always have
    /// `capacity` elements. If the underlying stream ended and only a partial
    /// vector was created, it'll be returned. Additionally if an error happens
    /// from the underlying stream then the currently buffered items will be
    /// yielded.
    ///
    /// This method is only available when the `std` or `alloc` feature of this
    /// library is activated, and it is activated by default.
    ///
    /// This function is similar to
    /// [`StreamExt::ready_chunks`](futures_util::stream::StreamExt::ready_chunks) but exits
    /// early if an error occurs.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    /// use tokio_stream_util::try_stream::TryReadyChunksError;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let stream = tokio_stream::iter(vec![
    ///         Ok::<i32, i32>(1), Ok(2), Ok(3), Err(4), Ok(5), Ok(6)
    ///     ]).try_ready_chunks(2);
    ///
    ///     let out = stream.into_stream().collect::<Vec<_>>().await;
    ///     assert_eq!(out, vec![
    ///         Ok(vec![1, 2]),
    ///         Ok(vec![3]),
    ///         Err(TryReadyChunksError::<i32, i32>(vec![], 4)),
    ///         Ok(vec![5, 6]),
    ///     ]);
    /// });
    /// ```
    ///
    /// # Panics
    ///
    /// This method will panic if `capacity` is zero.
    #[cfg(feature = "alloc")]
    fn try_ready_chunks(self, capacity: usize) -> TryReadyChunks<Self>
    where
        Self: Sized,
    {
        TryReadyChunks::new(self, capacity)
    }

    /// Attempt to filter the values produced by this stream according to the
    /// provided asynchronous closure.
    ///
    /// As values of this stream are made available, the provided predicate `f`
    /// will be run on them. If the predicate returns a `Future` which resolves
    /// to `true`, then the stream will yield the value, but if the predicate
    /// return a `Future` which resolves to `false`, then the value will be
    /// discarded and the next value will be produced.
    ///
    /// All errors are passed through without filtering in this combinator.
    ///
    /// Note that this function consumes the stream passed into it and returns a
    /// wrapped version of it, similar to the existing `filter` methods in
    /// the standard library.
    ///
    /// # Examples
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let events = tokio_stream::iter(vec![Ok::<i32, &str>(1), Ok(2), Ok(3), Err("error")])
    ///         .try_filter(|x| {
    ///             let v = *x;
    ///             async move { v >= 2 }
    ///         });
    ///
    ///     let out = events.into_stream().collect::<Vec<_>>().await;
    ///     assert_eq!(out, vec![Ok(2), Ok(3), Err("error")]);
    /// });
    /// ```
    fn try_filter<Fut, F>(self, f: F) -> TryFilter<Self, Fut, F>
    where
        Fut: Future<Output = bool>,
        F: FnMut(&Self::Ok) -> Fut,
        Self: Sized,
    {
        TryFilter::new(self, f)
    }

    /// Attempt to filter the values produced by this stream while
    /// simultaneously mapping them to a different type according to the
    /// provided asynchronous closure.
    ///
    /// As values of this stream are made available, the provided function will
    /// be run on them. If the future returned by the predicate `f` resolves to
    /// [`Some(item)`](Some) then the stream will yield the value `item`, but if
    /// it resolves to [`None`] then the next value will be produced.
    ///
    /// All errors are passed through without filtering in this combinator.
    ///
    /// Note that this function consumes the stream passed into it and returns a
    /// wrapped version of it, similar to the existing `filter_map` methods in
    /// the standard library.
    ///
    /// # Examples
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let halves = tokio_stream::iter(vec![Ok::<i32, &str>(1), Ok(6), Err("error")])
    ///         .try_filter_map(|x| async move {
    ///             let ret = if x % 2 == 0 { Some(x / 2) } else { None };
    ///             Ok(ret)
    ///         });
    ///
    ///     let out = halves.into_stream().collect::<Vec<_>>().await;
    ///     assert_eq!(out, vec![Ok(3), Err("error")]);
    /// });
    /// ```
    fn try_filter_map<Fut, F, T>(self, f: F) -> TryFilterMap<Self, Fut, F>
    where
        Fut: TryFuture<Ok = Option<T>, Error = Self::Error>,
        F: FnMut(Self::Ok) -> Fut,
        Self: Sized,
    {
        TryFilterMap::new(self, f)
    }

    /// Flattens a stream of streams into just one continuous stream. Produced streams
    /// will be polled concurrently and any errors will be passed through without looking at them.
    /// If the underlying base stream returns an error, it will be **immediately** propagated.
    ///
    /// The only argument is an optional limit on the number of concurrently
    /// polled streams. If this limit is not `None`, no more than `limit` streams
    /// will be polled at the same time. The `limit` argument is of type
    /// `Into<Option<usize>>`, and so can be provided as either `None`,
    /// `Some(10)`, or just `10`. Note: a limit of zero is interpreted as
    /// no limit at all, and will have the same result as passing in `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let inner1 = tokio_stream::iter(vec![Ok::<i32, i32>(1), Ok(2)]);
    ///     let inner2 = tokio_stream::iter(vec![Ok::<i32, i32>(3), Ok(4)]);
    ///     let base = tokio_stream::iter(vec![Ok::<_, i32>(inner1), Ok::<_, i32>(inner2)]);
    ///     let stream = base.try_flatten_unordered(None);
    ///
    ///     let mut out = stream.into_stream().collect::<Vec<_>>().await;
    ///     out.sort_by_key(|r| r.clone().unwrap_or_default());
    ///     assert_eq!(out, vec![Ok(1), Ok(2), Ok(3), Ok(4)]);
    /// });
    /// ```
    #[cfg(all(feature = "alloc", feature = "std"))]
    #[cfg_attr(target_os = "none", cfg(target_has_atomic = "ptr"))]
    fn try_flatten_unordered(self, limit: impl Into<Option<usize>>) -> TryFlattenUnordered<Self>
    where
        Self: Sized,
        Self::Ok: TryStream + Unpin,
        <Self::Ok as TryStream>::Error: From<Self::Error>,
    {
        TryFlattenUnordered::new(self, limit.into())
    }

    /// Flattens a stream of streams into just one continuous stream.
    ///
    /// If this stream's elements are themselves streams then this combinator
    /// will flatten out the entire stream to one long chain of elements. Any
    /// errors are passed through without looking at them, but otherwise each
    /// individual stream will get exhausted before moving on to the next.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let inner1 = tokio_stream::iter(vec![Ok::<i32, i32>(1), Ok(2)]);
    ///     let inner2 = tokio_stream::iter(vec![Ok::<i32, i32>(3), Ok(4)]);
    ///     let base = tokio_stream::iter(vec![Ok::<_, i32>(inner1), Ok::<_, i32>(inner2)]);
    ///     let stream = base.try_flatten();
    ///
    ///     let out = stream.into_stream().collect::<Vec<_>>().await;
    ///     assert_eq!(out, vec![Ok(1), Ok(2), Ok(3), Ok(4)]);
    /// });
    /// ```
    fn try_flatten(self) -> TryFlatten<Self>
    where
        Self::Ok: TryStream,
        <Self::Ok as TryStream>::Error: From<Self::Error>,
        Self: Sized,
    {
        TryFlatten::new(self)
    }

    /// Attempt to concatenate all items of a stream into a single
    /// extendable destination, returning a future representing the end result.
    ///
    /// This combinator will extend the first item with the contents of all
    /// the subsequent successful results of the stream. If the stream is empty,
    /// the default value will be returned.
    ///
    /// Works with all collections that implement the [`Extend`](core::iter::Extend) trait.
    ///
    /// This method is similar to [`concat`](futures_util::stream::StreamExt::concat), but will
    /// exit early if an error is encountered in the stream.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let fut = tokio_stream::iter(vec![
    ///         Ok::<Vec<i32>, ()>(vec![1, 2, 3]),
    ///         Ok(vec![4, 5, 6]),
    ///         Ok(vec![7, 8, 9]),
    ///     ]).try_concat();
    ///
    ///     assert_eq!(fut.await, Ok(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]));
    /// }
    /// ```
    fn try_concat(self) -> TryConcat<Self>
    where
        Self: Sized,
        Self::Ok: Extend<<<Self as TryStream>::Ok as IntoIterator>::Item> + IntoIterator + Default,
    {
        TryConcat::new(self)
    }

    /// Attempt to execute several futures from a stream concurrently (unordered).
    ///
    /// This stream's `Ok` type must be a [`TryFuture`](futures_core::future::TryFuture) with an `Error` type
    /// that matches the stream's `Error` type.
    ///
    /// This adaptor will buffer up to `n` futures and then return their
    /// outputs in the order in which they complete. If the underlying stream
    /// returns an error, it will be immediately propagated.
    ///
    /// The limit argument is of type `Into<Option<usize>>`, and so can be
    /// provided as either `None`, `Some(10)`, or just `10`. Note: a limit of zero is
    /// interpreted as no limit at all, and will have the same result as passing in `None`.
    ///
    /// The returned stream will be a stream of results, each containing either
    /// an error or a future's output. An error can be produced either by the
    /// underlying stream itself or by one of the futures it yielded.
    ///
    /// This method is only available when the `std` or `alloc` feature of this
    /// library is activated, and it is activated by default.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    ///   #[tokio::main]
    ///   async fn main() {
    ///     let unordered = tokio_stream::iter(vec![Ok::<i32, &str>(3), Ok(1), Ok(2)])
    ///         .map_ok(async |i| if i == 2 { Err("error") } else { Ok(i) })
    ///         .try_buffer_unordered(None);
    ///
    ///     let mut out = unordered.into_stream().collect::<Vec<_>>().await;
    ///     out.sort();
    ///     assert_eq!(out, vec![Ok(1), Ok(3), Err("error")]);
    /// }
    /// ```
    #[cfg(feature = "alloc")]
    #[cfg_attr(target_os = "none", cfg(target_has_atomic = "ptr"))]
    fn try_buffer_unordered(self, n: impl Into<Option<usize>>) -> TryBufferUnordered<Self>
    where
        Self::Ok: TryFuture<Error = Self::Error>,
        Self: Sized,
    {
        TryBufferUnordered::new(self, n.into())
    }

    /// Attempt to execute several futures from a stream concurrently.
    ///
    /// This stream's `Ok` type must be a [`TryFuture`](futures_core::future::TryFuture) with an `Error` type
    /// that matches the stream's `Error` type.
    ///
    /// This adaptor will buffer up to `n` futures and then return their
    /// outputs in the same order as the underlying stream. If the underlying stream returns an error, it will
    /// be immediately propagated.
    ///
    /// The limit argument is of type `Into<Option<usize>>`, and so can be
    /// provided as either `None`, `Some(10)`, or just `10`. Note: a limit of zero is
    /// interpreted as no limit at all, and will have the same result as passing in `None`.
    ///
    /// The returned stream will be a stream of results, each containing either
    /// an error or a future's output. An error can be produced either by the
    /// underlying stream itself or by one of the futures it yielded.
    ///
    /// This method is only available when the `std` or `alloc` feature of this
    /// library is activated, and it is activated by default.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let buffered = tokio_stream::iter(vec![Ok::<i32, &str>(3), Ok(1), Ok(2)])
    ///         .map_ok(|i| async move { if i == 2 { Err("error") } else { Ok(i) } })
    ///         .try_buffered(None);
    ///
    ///     let out = buffered.into_stream().collect::<Vec<_>>().await;
    ///     assert_eq!(out, vec![Ok(3), Ok(1), Err("error")]);
    /// }
    /// ```
    #[cfg(feature = "alloc")]
    #[cfg_attr(target_os = "none", cfg(target_has_atomic = "ptr"))]
    fn try_buffered(self, n: impl Into<Option<usize>>) -> TryBuffered<Self>
    where
        Self::Ok: TryFuture<Error = Self::Error>,
        Self: Sized,
    {
        TryBuffered::new(self, n.into())
    }

    /// A convenience method for calling [`TryStream::try_poll_next`] on [`Unpin`]
    /// stream types.
    fn try_poll_next_unpin(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Ok, Self::Error>>>
    where
        Self: Unpin,
    {
        Pin::new(self).try_poll_next(cx)
    }

    #[cfg(all(feature = "io", feature = "std"))]
    /// Adapter that converts this stream into an [`AsyncBufRead`](crate::io::AsyncBufRead).
    ///
    /// This method is only available when the `std` feature of this
    /// library is activated, and it is activated by default.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let reader = tokio_stream::iter([Ok::<_, std::io::Error>(vec![1, 2, 3])]).into_async_read();
    ///     let mut buf_reader = tokio::io::BufReader::new(reader);
    ///     let mut buf = Vec::new();
    ///     use tokio::io::AsyncReadExt;
    ///     buf_reader.read_to_end(&mut buf).await.unwrap();
    ///     assert_eq!(buf, vec![1, 2, 3]);
    /// });
    /// ```
    fn into_async_read(self) -> IntoAsyncRead<Self>
    where
        Self: Sized + TryStreamExt<Error = std::io::Error>,
        Self::Ok: AsRef<[u8]>,
    {
        IntoAsyncRead::new(self)
    }

    /// Attempt to execute a predicate over an asynchronous stream and evaluate if all items
    /// satisfy the predicate. Exits early if an `Err` is encountered or if an `Ok` item is found
    /// that does not satisfy the predicate.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let future_true = tokio_stream::iter(vec![1, 2, 3]).map(|i| Ok::<i32, &str>(i))
    ///         .try_all(|i| async move { i > 0 });
    ///     assert!(future_true.await.unwrap());
    ///
    ///     let future_err = tokio_stream::iter(vec![Ok::<i32, &str>(1), Err("err"), Ok(3)])
    ///         .try_all(|i| async move { i > 0 });
    ///     assert!(future_err.await.is_err());
    /// });
    /// ```
    fn try_all<Fut, F>(self, f: F) -> TryAll<Self, Fut, F>
    where
        Self: Sized,
        F: FnMut(Self::Ok) -> Fut,
        Fut: Future<Output = bool>,
    {
        TryAll::new(self, f)
    }

    /// Attempt to execute a predicate over an asynchronous stream and evaluate if any items
    /// satisfy the predicate. Exits early if an `Err` is encountered or if an `Ok` item is found
    /// that satisfies the predicate.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_stream::StreamExt;
    /// use tokio_stream_util::TryStreamExt;
    ///
    /// let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    /// rt.block_on(async {
    ///     let future_true = tokio_stream::iter(0..10).map(|i| Ok::<i32, &str>(i))
    ///         .try_any(|i| async move { i == 3 });
    ///     assert!(future_true.await.unwrap());
    ///
    ///     let future_err = tokio_stream::iter(vec![Ok::<i32, &str>(1), Err("err"), Ok(3)])
    ///         .try_any(|i| async move { i == 3 });
    ///     assert!(future_err.await.is_err());
    /// });
    /// ```
    fn try_any<Fut, F>(self, f: F) -> TryAny<Self, Fut, F>
    where
        Self: Sized,
        F: FnMut(Self::Ok) -> Fut,
        Fut: Future<Output = bool>,
    {
        TryAny::new(self, f)
    }
}
