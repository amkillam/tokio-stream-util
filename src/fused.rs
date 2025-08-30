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
