use core::{
    pin::Pin,
    result::Result,
    task::{Context, Poll},
};
use tokio_stream::Stream;

mod private_try_stream {
    use super::Stream;

    pub trait Sealed {}

    impl<S, T, E> Sealed for S where S: ?Sized + Stream<Item = Result<T, E>> {}
}

/// A convenience for streams that return `Result` values that includes
/// a variety of adapters tailored to such futures.
pub trait TryStream: Stream + private_try_stream::Sealed {
    /// The type of successful values yielded by this future
    type Ok;

    /// The type of failures yielded by this future
    type Error;

    /// Poll this `TryStream` as if it were a `Stream`.
    ///
    /// This method is a stopgap for a compiler limitation that prevents us from
    /// directly inheriting from the `Stream` trait; in the future it won't be
    /// needed.
    fn try_poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Ok, Self::Error>>>;
}

impl<S, T, E> TryStream for S
where
    S: ?Sized + Stream<Item = Result<T, E>>,
{
    type Ok = T;
    type Error = E;

    fn try_poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Ok, Self::Error>>> {
        self.poll_next(cx)
    }
}
