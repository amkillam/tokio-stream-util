//! Stream for the [`flatten_unordered`](super::StreamExt::flatten_unordered)
//! method with ability to specify flow controller.
//! ! This is a more generic version of `FlattenUnordered` which allows to
//! ! control the flow of items from the base stream to the inner streams.
//! ! The main use-case is to immediately return an item from the base stream
//! //! ! without adding it to the inner streams bucket.
//!
//! # Examples
//! ```
//! use tokio_stream::StreamExt;
//! use tokio_stream_util::FlattenUnordered;
//!
//! #[tokio::main]
//! async fn main() {
//!  let unordered_stream = tokio_stream::iter(vec![
//!    tokio_stream::iter(vec![1, 2, 3]),
//!    tokio_stream::iter(vec![4, 5, 6]),
//!    tokio_stream::iter(vec![7, 8, 9]),
//!    tokio_stream::iter(vec![10, 11, 12]),
//!    tokio_stream::iter(vec![13, 14, 15]),
//!    tokio_stream::iter(vec![16, 17, 18]),
//!    tokio_stream::iter(vec![19, 20, 21]),
//!    tokio_stream::iter(vec![22, 23, 24]),
//!    tokio_stream::iter(vec![25, 26, 27]),
//!    tokio_stream::iter(vec![28, 29, 30]),
//!    ]);
//!    let mut stream = FlattenUnordered::new(unordered_stream, Some(3));
//!    let mut expected_finds = std::collections::HashMap::new();
//!    for i in 1..=30 {
//!    expected_finds.insert(i, false);
//!    }
//!
//!
//!    while let Some(item) = stream.next().await {
//!     assert!(item >= 1 && item <= 30);
//!     *expected_finds.get_mut(&item).unwrap() = true;
//!    }
//!
//!    for (key, found) in expected_finds {
//!    assert!(found, "Item {} was not found in the stream", key);
//!    }
//! }
//!
//! ```
//!

use alloc::sync::Arc;
use core::{
    cell::UnsafeCell,
    convert::identity,
    fmt,
    future::Future,
    marker::PhantomData,
    num::NonZeroUsize,
    pin::Pin,
    sync::atomic::{AtomicU8, Ordering},
    task::{Context, Poll, Waker},
};

use crate::FusedStream;
#[cfg(feature = "sink")]
use async_sink::Sink;
use futures_task::{waker, ArcWake};
use tokio_stream::Stream;

use crate::FuturesUnordered;

/// Stream for the [`flatten_unordered`](super::StreamExt::flatten_unordered)
/// method.
pub type FlattenUnordered<St> = FlattenUnorderedWithFlowController<St, ()>;

/// There is nothing to poll and stream isn't being polled/waking/woken at the moment.
const NONE: u8 = 0;

/// Inner streams need to be polled.
const NEED_TO_POLL_INNER_STREAMS: u8 = 1;

/// The base stream needs to be polled.
const NEED_TO_POLL_STREAM: u8 = 0b10;

/// Both base stream and inner streams need to be polled.
const NEED_TO_POLL_ALL: u8 = NEED_TO_POLL_INNER_STREAMS | NEED_TO_POLL_STREAM;

/// The current stream is being polled at the moment.
const POLLING: u8 = 0b100;

/// Stream is being woken at the moment.
const WAKING: u8 = 0b1000;

/// The stream was waked and will be polled.
const WOKEN: u8 = 0b10000;

/// Internal polling state of the stream.
#[derive(Clone, Debug)]
struct SharedPollState {
    state: Arc<AtomicU8>,
}

impl SharedPollState {
    /// Constructs new `SharedPollState` with the given state.
    fn new(value: u8) -> Self {
        Self {
            state: Arc::new(AtomicU8::new(value)),
        }
    }

    /// Attempts to start polling, returning stored state in case of success.
    /// Returns `None` if either waker is waking at the moment.
    fn start_polling(&self) -> Option<(u8, PollStateBomb<'_, impl FnOnce(&Self) -> u8>)> {
        let value = self
            .state
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                if value & WAKING == NONE {
                    Some(POLLING)
                } else {
                    None
                }
            })
            .ok()?;
        let bomb = PollStateBomb::new(self, Self::reset);

        Some((value, bomb))
    }

    /// Attempts to start the waking process and performs bitwise or with the given value.
    ///
    /// If some waker is already in progress or stream is already woken/being polled, waking process won't start, however
    /// state will be disjuncted with the given value.
    fn start_waking(
        &self,
        to_poll: u8,
    ) -> Option<(u8, PollStateBomb<'_, impl FnOnce(&SharedPollState) -> u8>)> {
        let value = self
            .state
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                let mut next_value = value | to_poll;
                if value & (WOKEN | POLLING) == NONE {
                    next_value |= WAKING;
                }

                if next_value != value {
                    Some(next_value)
                } else {
                    None
                }
            })
            .ok()?;

        // Only start the waking process if we're not in the polling/waking phase and the stream isn't woken already
        if value & (WOKEN | POLLING | WAKING) == NONE {
            let bomb = PollStateBomb::new(self, Self::stop_waking);

            Some((value, bomb))
        } else {
            None
        }
    }

    /// Sets current state to
    /// - `!POLLING` allowing to use wakers
    /// - `WOKEN` if the state was changed during `POLLING` phase as waker will be called,
    ///   or `will_be_woken` flag supplied
    /// - `!WAKING` as
    ///   * Wakers called during the `POLLING` phase won't propagate their calls
    ///   * `POLLING` phase can't start if some of the wakers are active
    ///     So no wrapped waker can touch the inner waker's cell, it's safe to poll again.
    fn stop_polling(&self, to_poll: u8, will_be_woken: bool) -> u8 {
        self.state
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |mut value| {
                let mut next_value = to_poll;

                value &= NEED_TO_POLL_ALL;
                if value != NONE || will_be_woken {
                    next_value |= WOKEN;
                }
                next_value |= value;

                Some(next_value & !POLLING & !WAKING)
            })
            .unwrap()
    }

    /// Toggles state to non-waking, allowing to start polling.
    fn stop_waking(&self) -> u8 {
        let value = self
            .state
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                let next_value = value & !WAKING | WOKEN;

                if next_value != value {
                    Some(next_value)
                } else {
                    None
                }
            })
            .unwrap_or_else(identity);

        debug_assert!(value & (WOKEN | POLLING | WAKING) == WAKING);
        value
    }

    /// Resets current state allowing to poll the stream and wake up wakers.
    fn reset(&self) -> u8 {
        self.state.swap(NEED_TO_POLL_ALL, Ordering::SeqCst)
    }
}

/// Used to execute some function on the given state when dropped.
struct PollStateBomb<'a, F: FnOnce(&SharedPollState) -> u8> {
    state: &'a SharedPollState,
    drop: Option<F>,
}

impl<'a, F: FnOnce(&SharedPollState) -> u8> PollStateBomb<'a, F> {
    /// Constructs new bomb with the given state.
    fn new(state: &'a SharedPollState, drop: F) -> Self {
        Self {
            state,
            drop: Some(drop),
        }
    }

    /// Deactivates bomb, forces it to not call provided function when dropped.
    fn deactivate(mut self) {
        self.drop.take();
    }
}

impl<F: FnOnce(&SharedPollState) -> u8> Drop for PollStateBomb<'_, F> {
    fn drop(&mut self) {
        if let Some(drop) = self.drop.take() {
            (drop)(self.state);
        }
    }
}

/// Will update state with the provided value on `wake_by_ref` call
/// and then, if there is a need, call `inner_waker`.
struct WrappedWaker {
    inner_waker: UnsafeCell<Option<Waker>>,
    poll_state: SharedPollState,
    need_to_poll: u8,
}

unsafe impl Send for WrappedWaker {}
unsafe impl Sync for WrappedWaker {}

impl WrappedWaker {
    /// Replaces given waker's inner_waker for polling stream/futures which will
    /// update poll state on `wake_by_ref` call. Use only if you need several
    /// contexts.
    ///
    /// ## Safety
    ///
    /// This function will modify waker's `inner_waker` via `UnsafeCell`, so
    /// it should be used only during `POLLING` phase by one thread at the time.
    unsafe fn replace_waker(self_arc: &mut Arc<Self>, cx: &Context<'_>) {
        unsafe { *self_arc.inner_waker.get() = cx.waker().clone().into() }
    }

    /// Attempts to start the waking process for the waker with the given value.
    /// If succeeded, then the stream isn't yet woken and not being polled at the moment.
    fn start_waking(&self) -> Option<(u8, PollStateBomb<'_, impl FnOnce(&SharedPollState) -> u8>)> {
        self.poll_state.start_waking(self.need_to_poll)
    }
}

impl ArcWake for WrappedWaker {
    fn wake_by_ref(self_arc: &Arc<Self>) {
        if let Some((_, state_bomb)) = self_arc.start_waking() {
            // Safety: now state is not `POLLING`
            let waker_opt = unsafe { self_arc.inner_waker.get().as_ref().unwrap() };

            if let Some(inner_waker) = waker_opt.clone() {
                // Stop waking to allow polling stream
                drop(state_bomb);

                // Wake up inner waker
                inner_waker.wake();
            }
        }
    }
}

/// Future which polls optional inner stream.
///
/// If it's `Some`, it will attempt to call `poll_next` on it,
/// returning `Some((item, next_item_fut))` in case of `Poll::Ready(Some(...))`
/// or `None` in case of `Poll::Ready(None)`.
///
/// If `poll_next` will return `Poll::Pending`, it will be forwarded to
/// the future and current task will be notified by waker.
#[must_use = "futures do nothing unless you `.await` or poll them"]
struct PollStreamFut<St> {
    stream: Option<St>,
}

/// Projection returned by PollStreamFut::project
struct PollStreamFutProj<'pin, St: Sized> {
    pub stream: Pin<&'pin mut Option<St>>,
}

impl<St> PollStreamFut<St> {
    /// Constructs new `PollStreamFut` using given `stream`.
    fn new(stream: impl Into<Option<St>>) -> Self {
        Self {
            stream: stream.into(),
        }
    }

    /// Project a `Pin<&mut PollStreamFut<St>>` to its fields.
    fn project<'pin>(self: Pin<&'pin mut Self>) -> PollStreamFutProj<'pin, St> {
        unsafe {
            let Self { stream } = Pin::get_unchecked_mut(self);
            PollStreamFutProj {
                stream: Pin::new_unchecked(stream),
            }
        }
    }
}

impl<St: Stream + Unpin> Future for PollStreamFut<St> {
    type Output = Option<(St::Item, Self)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut stream = self.project().stream;

        let item = if let Some(stream) = stream.as_mut().as_pin_mut() {
            match stream.poll_next(cx) {
                Poll::Ready(Some(item)) => Some(item),
                Poll::Ready(None) => None,
                Poll::Pending => return Poll::Pending,
            }
        } else {
            None
        };
        let next_item_fut = Self::new(stream.get_mut().take());
        let out = item.map(|item| (item, next_item_fut));

        Poll::Ready(out)
    }
}

/// Stream for the [`flatten_unordered`](super::StreamExt::flatten_unordered)
/// method with ability to specify flow controller.
#[must_use = "streams do nothing unless polled"]
pub struct FlattenUnorderedWithFlowController<St, Fc>
where
    St: Stream,
{
    inner_streams: FuturesUnordered<PollStreamFut<St::Item>>,
    stream: St,
    poll_state: SharedPollState,
    limit: Option<NonZeroUsize>,
    is_stream_done: bool,
    inner_streams_waker: Arc<WrappedWaker>,
    stream_waker: Arc<WrappedWaker>,
    flow_controller: PhantomData<Fc>,
}

/// Projection struct for FlattenUnorderedWithFlowController
pub(crate) struct FlattenUnorderedWithFlowControllerProj<'pin, St: ?Sized + Stream, Fc: ?Sized> {
    inner_streams: Pin<&'pin mut FuturesUnordered<PollStreamFut<<St as Stream>::Item>>>,
    stream: Pin<&'pin mut St>,
    poll_state: &'pin mut SharedPollState,
    limit: &'pin mut Option<NonZeroUsize>,
    is_stream_done: &'pin mut bool,
    inner_streams_waker: &'pin mut Arc<WrappedWaker>,
    stream_waker: &'pin mut Arc<WrappedWaker>,
    #[allow(dead_code)]
    flow_controller: &'pin mut PhantomData<Fc>,
}

impl<St, Fc> fmt::Debug for FlattenUnorderedWithFlowController<St, Fc>
where
    St: Stream + fmt::Debug,
    St::Item: Stream + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlattenUnorderedWithFlowController")
            .field("poll_state", &self.poll_state)
            .field("inner_streams", &self.inner_streams)
            .field("limit", &self.limit)
            .field("stream", &self.stream)
            .field("is_stream_done", &self.is_stream_done)
            .field("flow_controller", &self.flow_controller)
            .finish()
    }
}

impl<St, Fc> FlattenUnorderedWithFlowController<St, Fc>
where
    St: Stream,
    Fc: FlowController<St::Item, <St::Item as Stream>::Item>,
    St::Item: Stream + Unpin,
{
    /// Creates a new `FlattenUnorderedWithFlowController` stream.
    pub fn new(stream: St, limit: Option<usize>) -> Self {
        let poll_state = SharedPollState::new(NEED_TO_POLL_STREAM);

        Self {
            inner_streams: FuturesUnordered::new(),
            stream,
            is_stream_done: false,
            limit: limit.and_then(NonZeroUsize::new),
            inner_streams_waker: Arc::new(WrappedWaker {
                inner_waker: UnsafeCell::new(None),
                poll_state: poll_state.clone(),
                need_to_poll: NEED_TO_POLL_INNER_STREAMS,
            }),
            stream_waker: Arc::new(WrappedWaker {
                inner_waker: UnsafeCell::new(None),
                poll_state: poll_state.clone(),
                need_to_poll: NEED_TO_POLL_STREAM,
            }),
            poll_state,
            flow_controller: PhantomData,
        }
    }

    /// Returns a reference to the underlying stream.
    pub fn get_ref(&self) -> &St {
        &self.stream
    }

    /// Returns a mutable reference to the underlying stream.
    pub fn get_mut(&mut self) -> &mut St {
        &mut self.stream
    }

    /// Returns a pinned mutable reference to the underlying stream.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        // Safety: stream field is pinned in the struct definition.
        unsafe { Pin::new_unchecked(&mut Pin::get_unchecked_mut(self).stream) }
    }

    /// Consumes this wrapper and returns the underlying stream.
    pub fn into_inner(self) -> St {
        self.stream
    }

    pub(crate) fn project<'pin>(
        self: Pin<&'pin mut Self>,
    ) -> FlattenUnorderedWithFlowControllerProj<'pin, St, Fc> {
        unsafe {
            let this = self.get_unchecked_mut();
            FlattenUnorderedWithFlowControllerProj {
                inner_streams: Pin::new_unchecked(&mut this.inner_streams),
                stream: Pin::new_unchecked(&mut this.stream),
                poll_state: &mut this.poll_state,
                limit: &mut this.limit,
                is_stream_done: &mut this.is_stream_done,
                inner_streams_waker: &mut this.inner_streams_waker,
                stream_waker: &mut this.stream_waker,
                flow_controller: &mut this.flow_controller,
            }
        }
    }
}

/// Returns the next flow step based on the received item.
pub trait FlowController<I, O> {
    /// Handles an item producing `FlowStep` describing the next flow step.
    fn next_step(item: I) -> FlowStep<I, O>;
}

impl<I, O> FlowController<I, O> for () {
    fn next_step(item: I) -> FlowStep<I, O> {
        FlowStep::Continue(item)
    }
}

/// Describes the next flow step.
#[derive(Debug, Clone)]
pub enum FlowStep<C, R> {
    /// Just yields an item and continues standard flow.
    Continue(C),
    /// Immediately returns an underlying item from the function.
    Return(R),
}

impl<St, Fc> FlattenUnorderedWithFlowControllerProj<'_, St, Fc>
where
    St: Stream,
{
    /// Checks if current `inner_streams` bucket size is greater than optional limit.
    fn is_exceeded_limit(&self) -> bool {
        self.limit.map_or(false, |limit| {
            // Use Pin::get_ref to obtain &FuturesUnordered
            self.inner_streams.as_ref().len() >= limit.get()
        })
    }
}

impl<St, Fc> FusedStream for FlattenUnorderedWithFlowController<St, Fc>
where
    St: FusedStream,
    Fc: FlowController<St::Item, <St::Item as Stream>::Item>,
    St::Item: Stream + Unpin,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated() && self.inner_streams.is_empty()
    }
}

impl<St, Fc> Stream for FlattenUnorderedWithFlowController<St, Fc>
where
    St: Stream,
    Fc: FlowController<St::Item, <St::Item as Stream>::Item>,
    St::Item: Stream + Unpin,
{
    type Item = <St::Item as Stream>::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut next_item = None;
        let mut need_to_poll_next = NONE;

        // Project fields
        // Safety: mirror of pin-project generated projection
        let mut this = self.project();

        // Attempt to start polling, in case some waker is holding the lock, wait in loop
        let (mut poll_state_value, state_bomb) = loop {
            if let Some(value) = this.poll_state.start_polling() {
                break value;
            }
        };

        // Safety: now state is `POLLING`.
        unsafe {
            WrappedWaker::replace_waker(this.stream_waker, cx);
            WrappedWaker::replace_waker(this.inner_streams_waker, cx)
        };

        if poll_state_value & NEED_TO_POLL_STREAM != NONE {
            let mut stream_waker = None;

            // Here we need to poll the base stream.
            //
            // To improve performance, we will attempt to place as many items as we can
            // to the `FuturesUnordered` bucket before polling inner streams
            loop {
                if this.is_exceeded_limit() || *this.is_stream_done {
                    // We either exceeded the limit or the stream is exhausted
                    if !*this.is_stream_done {
                        // The stream needs to be polled in the next iteration
                        need_to_poll_next |= NEED_TO_POLL_STREAM;
                    }

                    break;
                } else {
                    let mut cx = Context::from_waker(
                        stream_waker.get_or_insert_with(|| waker(this.stream_waker.clone())),
                    );

                    match this.stream.as_mut().poll_next(&mut cx) {
                        Poll::Ready(Some(item)) => {
                            let next_item_fut = match Fc::next_step(item) {
                                // Propagates an item immediately (the main use-case is for errors)
                                FlowStep::Return(item) => {
                                    need_to_poll_next |= NEED_TO_POLL_STREAM
                                        | (poll_state_value & NEED_TO_POLL_INNER_STREAMS);
                                    poll_state_value &= !NEED_TO_POLL_INNER_STREAMS;

                                    next_item = Some(item);

                                    break;
                                }
                                // Yields an item and continues processing (normal case)
                                FlowStep::Continue(inner_stream) => {
                                    PollStreamFut::new(inner_stream)
                                }
                            };
                            // Add new stream to the inner streams bucket
                            this.inner_streams.as_mut().push(next_item_fut);
                            // Inner streams must be polled afterward
                            poll_state_value |= NEED_TO_POLL_INNER_STREAMS;
                        }
                        Poll::Ready(None) => {
                            // Mark the base stream as done
                            *this.is_stream_done = true;
                        }
                        Poll::Pending => {
                            break;
                        }
                    }
                }
            }
        }

        if poll_state_value & NEED_TO_POLL_INNER_STREAMS != NONE {
            let inner_streams_waker = waker(this.inner_streams_waker.clone());
            let mut cx = Context::from_waker(&inner_streams_waker);

            match this.inner_streams.as_mut().poll_next(&mut cx) {
                Poll::Ready(Some(Some((item, next_item_fut)))) => {
                    // Push next inner stream item future to the list of inner streams futures
                    this.inner_streams.as_mut().push(next_item_fut);
                    // Take the received item
                    next_item = Some(item);
                    // On the next iteration, inner streams must be polled again
                    need_to_poll_next |= NEED_TO_POLL_INNER_STREAMS;
                }
                Poll::Ready(Some(None)) => {
                    // On the next iteration, inner streams must be polled again
                    need_to_poll_next |= NEED_TO_POLL_INNER_STREAMS;
                }
                _ => {}
            }
        }

        // We didn't have any `poll_next` panic, so it's time to deactivate the bomb
        state_bomb.deactivate();

        // Call the waker at the end of polling if
        let mut force_wake =
            // we need to poll the stream and didn't reach the limit yet
            need_to_poll_next & NEED_TO_POLL_STREAM != NONE && !this.is_exceeded_limit()
            // or we need to poll the inner streams again
            || need_to_poll_next & NEED_TO_POLL_INNER_STREAMS != NONE;

        // Stop polling and swap the latest state
        poll_state_value = this.poll_state.stop_polling(need_to_poll_next, force_wake);
        // If state was changed during `POLLING` phase, we also need to manually call a waker
        force_wake |= poll_state_value & NEED_TO_POLL_ALL != NONE;

        let is_done = *this.is_stream_done && this.inner_streams.is_empty();

        if next_item.is_some() || is_done {
            Poll::Ready(next_item)
        } else {
            if force_wake {
                cx.waker().wake_by_ref();
            }

            Poll::Pending
        }
    }
}

// Forwarding impl of Sink from the underlying stream
#[cfg(feature = "sink")]
impl<St, Item, Fc> Sink<Item> for FlattenUnorderedWithFlowController<St, Fc>
where
    St: Stream + Sink<Item>,
{
    type Error = St::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // project and delegate to inner stream
        let this = unsafe {
            let s: *mut FlattenUnorderedWithFlowController<St, Fc> = Pin::get_unchecked_mut(self);
            FlattenUnorderedWithFlowControllerProj {
                inner_streams: Pin::new_unchecked(&mut (*s).inner_streams),
                stream: Pin::new_unchecked(&mut (*s).stream),
                poll_state: &mut (*s).poll_state,
                limit: &mut (*s).limit,
                is_stream_done: &mut (*s).is_stream_done,
                inner_streams_waker: &mut (*s).inner_streams_waker,
                stream_waker: &mut (*s).stream_waker,
                flow_controller: &mut (*s).flow_controller,
            }
        };
        this.stream.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let this = unsafe {
            let s: *mut FlattenUnorderedWithFlowController<St, Fc> = Pin::get_unchecked_mut(self);
            FlattenUnorderedWithFlowControllerProj {
                inner_streams: Pin::new_unchecked(&mut (*s).inner_streams),
                stream: Pin::new_unchecked(&mut (*s).stream),
                poll_state: &mut (*s).poll_state,
                limit: &mut (*s).limit,
                is_stream_done: &mut (*s).is_stream_done,
                inner_streams_waker: &mut (*s).inner_streams_waker,
                stream_waker: &mut (*s).stream_waker,
                flow_controller: &mut (*s).flow_controller,
            }
        };
        this.stream.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = unsafe {
            let s: *mut FlattenUnorderedWithFlowController<St, Fc> = Pin::get_unchecked_mut(self);
            FlattenUnorderedWithFlowControllerProj {
                inner_streams: Pin::new_unchecked(&mut (*s).inner_streams),
                stream: Pin::new_unchecked(&mut (*s).stream),
                poll_state: &mut (*s).poll_state,
                limit: &mut (*s).limit,
                is_stream_done: &mut (*s).is_stream_done,
                inner_streams_waker: &mut (*s).inner_streams_waker,
                stream_waker: &mut (*s).stream_waker,
                flow_controller: &mut (*s).flow_controller,
            }
        };
        this.stream.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = unsafe {
            let s: *mut FlattenUnorderedWithFlowController<St, Fc> = Pin::get_unchecked_mut(self);
            FlattenUnorderedWithFlowControllerProj {
                inner_streams: Pin::new_unchecked(&mut (*s).inner_streams),
                stream: Pin::new_unchecked(&mut (*s).stream),
                poll_state: &mut (*s).poll_state,
                limit: &mut (*s).limit,
                is_stream_done: &mut (*s).is_stream_done,
                inner_streams_waker: &mut (*s).inner_streams_waker,
                stream_waker: &mut (*s).stream_waker,
                flow_controller: &mut (*s).flow_controller,
            }
        };
        this.stream.poll_close(cx)
    }
}
