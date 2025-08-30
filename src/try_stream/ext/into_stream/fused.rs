use core::{
    fmt,
    pin::Pin,
    sync::atomic::AtomicBool,
    task::{Context, Poll},
};

use futures_core::ready;

use crate::{try_stream::IntoStream, TryStream};
use tokio_stream::Stream;

/// Stream returned by `fuse()`.
///
pub struct IntoFuseStream<St> {
    stream: IntoStream<St>,
    terminated: AtomicBool,
}

/// Projection returned by `IntoFuseStream::project`.
pub struct IntoFuseStreamProj<'pin, St> {
    pub stream: Pin<&'pin mut IntoStream<St>>,
    pub terminated: &'pin AtomicBool,
}

impl<St> IntoFuseStream<St> {
    pub(crate) fn new(stream: St) -> Self {
        Self {
            stream: IntoStream::new(stream),
            terminated: AtomicBool::new(false),
        }
    }
}

impl<St> IntoFuseStream<St> {
    pub fn get_ref(&self) -> &St {
        self.stream.get_ref()
    }

    pub fn get_mut(&mut self) -> &mut St {
        self.stream.get_mut()
    }

    pub fn into_inner(self) -> St {
        self.stream.into_inner()
    }

    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        unsafe { self.map_unchecked_mut(|s| &mut s.stream.stream) }
    }

    /// Project a `Pin<&mut IntoFuseStream<St>>` to its fields.
    pub(crate) fn project<'pin>(self: Pin<&'pin mut Self>) -> IntoFuseStreamProj<'pin, St> {
        unsafe {
            let this = self.get_unchecked_mut();
            IntoFuseStreamProj {
                stream: Pin::new_unchecked(&mut this.stream),
                terminated: &this.terminated,
            }
        }
    }
}

impl<St: fmt::Debug> fmt::Debug for IntoFuseStream<St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IntoFuseStream")
            .field("stream", &self.stream)
            .finish()
    }
}

impl<St> Stream for IntoFuseStream<St>
where
    St: TryStream,
    IntoStream<St>: Stream,
{
    type Item = <IntoStream<St> as tokio_stream::Stream>::Item;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.terminated.load(core::sync::atomic::Ordering::SeqCst) {
            Poll::Ready(None)
        } else {
            let projected = self.as_mut().project();
            match ready!(projected.stream.poll_next(cx)) {
                Some(item) => Poll::Ready(Some(item)),

                None => {
                    // Do not poll the stream anymore
                    self.terminated
                        .store(true, core::sync::atomic::Ordering::SeqCst);
                    Poll::Ready(None)
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

impl<St> crate::FusedStream for IntoFuseStream<St>
where
    St: TryStream,
{
    fn is_terminated(&self) -> bool {
        self.terminated.load(core::sync::atomic::Ordering::SeqCst)
    }
}
