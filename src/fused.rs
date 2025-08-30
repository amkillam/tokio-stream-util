//! A stream which can report whether or not it has terminated.
//! !!
//! This is similar to the `FusedFuture` trait in the standard library.
//! However, unlike `FusedFuture`, this trait is not implemented for all
//! streams, only those which explicitly opt in to it.
//! This is because not all streams can meaningfully report whether or not
//! they have terminated.
//! !!! This trait is primarily useful for combinators which need to
//! track whether or not a stream has terminated.
//! !!! It is not intended for general use.
//! !!! For a stream that is guaranteed to terminate, use the `StreamExt::fuse`
//! !!! method to create a stream that implements `FusedStream`.

use core::{ops::DerefMut, pin::Pin};
use tokio_stream::Stream;

/// A stream which tracks whether or not the underlying stream
/// should no longer be polled.
///
/// `is_terminated` will return `true` if a future should no longer be polled.
/// Usually, this state occurs after `poll_next` (or `try_poll_next`) returned
/// `Poll::Ready(None)`. However, `is_terminated` may also return `true` if a
/// stream has become inactive and can no longer make progress and should be
/// ignored or dropped rather than being polled again.
pub trait FusedStream: Stream {
    /// Returns `true` if the stream should no longer be polled.
    fn is_terminated(&self) -> bool;
}

impl<F: ?Sized + FusedStream + Unpin> FusedStream for &mut F {
    fn is_terminated(&self) -> bool {
        <F as FusedStream>::is_terminated(&**self)
    }
}

impl<P> FusedStream for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: FusedStream,
{
    fn is_terminated(&self) -> bool {
        <P::Target as FusedStream>::is_terminated(&**self)
    }
}
