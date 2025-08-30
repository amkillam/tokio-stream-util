use core::future::Future;
use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::FusedFuture;

use super::{FusedStream, TryStream};

/// Future for the [`try_concat`](super::TryStreamExt::try_concat) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct TryConcat<St: TryStream> {
    stream: St,
    accum: Option<St::Ok>,
}

impl<St: TryStream + Unpin> Unpin for TryConcat<St> {}

impl<St> TryConcat<St>
where
    St: TryStream,
    St::Ok: Extend<<St::Ok as IntoIterator>::Item> + IntoIterator + Default,
{
    pub(super) fn new(stream: St) -> Self {
        Self {
            stream,
            accum: None,
        }
    }
}

impl<St> FusedFuture for TryConcat<St>
where
    St: TryStream + FusedStream,
    St::Ok: Extend<<St::Ok as IntoIterator>::Item> + IntoIterator + Default,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated() && self.accum.is_none()
    }
}

impl<St> Future for TryConcat<St>
where
    St: TryStream,
    St::Ok: Extend<<St::Ok as IntoIterator>::Item> + IntoIterator + Default,
{
    type Output = Result<St::Ok, St::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };

        loop {
            match stream.as_mut().try_poll_next(cx) {
                Poll::Ready(Some(Ok(x))) => {
                    if let Some(a) = &mut this.accum {
                        a.extend(x);
                    } else {
                        this.accum = Some(x);
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                Poll::Ready(None) => {
                    return Poll::Ready(Ok(mem::take(&mut this.accum).unwrap_or_default()))
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
