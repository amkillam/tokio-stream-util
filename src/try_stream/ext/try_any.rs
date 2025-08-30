use core::fmt;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::FusedFuture;

use super::TryStream;

/// Future for the [`try_any`](super::TryStreamExt::try_any) method.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct TryAny<St, Fut, F> {
    stream: St,
    f: F,
    done: bool,
    future: Option<Fut>,
}

impl<St: Unpin, Fut: Unpin, F> Unpin for TryAny<St, Fut, F> {}

impl<St, Fut, F> TryAny<St, Fut, F> {
    // Safety: The caller must not move the fields of `this` until the returned `Pin`s are dropped.
    unsafe fn project(
        self: Pin<&mut Self>,
    ) -> (Pin<&mut St>, &mut F, &mut bool, Pin<&mut Option<Fut>>) {
        let this = self.get_unchecked_mut();
        (
            Pin::new_unchecked(&mut this.stream),
            &mut this.f,
            &mut this.done,
            Pin::new_unchecked(&mut this.future),
        )
    }
}

impl<St, Fut, F> fmt::Debug for TryAny<St, Fut, F>
where
    St: fmt::Debug,
    Fut: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TryAny")
            .field("stream", &self.stream)
            .field("done", &self.done)
            .field("future", &self.future)
            .finish()
    }
}

impl<St, Fut, F> TryAny<St, Fut, F>
where
    St: TryStream,
    F: FnMut(St::Ok) -> Fut,
    Fut: Future<Output = bool>,
{
    pub(super) fn new(stream: St, f: F) -> Self {
        Self {
            stream,
            f,
            done: false,
            future: None,
        }
    }
}

impl<St, Fut, F> FusedFuture for TryAny<St, Fut, F>
where
    St: TryStream,
    F: FnMut(St::Ok) -> Fut,
    Fut: Future<Output = bool>,
{
    fn is_terminated(&self) -> bool {
        self.done && self.future.is_none()
    }
}

impl<St, Fut, F> Future for TryAny<St, Fut, F>
where
    St: TryStream,
    F: FnMut(St::Ok) -> Fut,
    Fut: Future<Output = bool>,
{
    type Output = Result<bool, St::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<bool, St::Error>> {
        let (mut stream, f, done, mut future) = unsafe { self.project() };
        loop {
            if let Some(fut) = future.as_mut().as_pin_mut() {
                // we're currently processing a future to produce a new value
                let acc = match fut.poll(cx) {
                    Poll::Ready(x) => x,
                    Poll::Pending => return Poll::Pending,
                };
                future.set(None);
                if acc {
                    *done = true;
                    return Poll::Ready(Ok(true));
                } // early exit
            } else if !*done {
                // we're waiting on a new item from the stream
                match stream.as_mut().try_poll_next(cx) {
                    Poll::Ready(Some(Ok(item))) => {
                        future.set(Some(f(item)));
                    }
                    Poll::Ready(Some(Err(err))) => {
                        *done = true;
                        return Poll::Ready(Err(err));
                    }
                    Poll::Ready(None) => {
                        *done = true;
                        return Poll::Ready(Ok(false));
                    }
                    Poll::Pending => {
                        return Poll::Pending;
                    }
                }
            } else {
                panic!("TryAny polled after completion")
            }
        }
    }
}
