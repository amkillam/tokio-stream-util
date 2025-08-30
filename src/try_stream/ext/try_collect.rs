use core::future::Future;
use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::FusedFuture;

use super::{FusedStream, TryStream};

/// Future for the [`try_collect`](super::TryStreamExt::try_collect) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct TryCollect<St, C> {
    stream: St,
    items: C,
}

impl<St: TryStream, C: Default> TryCollect<St, C> {
    pub(super) fn new(s: St) -> Self {
        Self {
            stream: s,
            items: Default::default(),
        }
    }
}

impl<St, C> FusedFuture for TryCollect<St, C>
where
    St: TryStream + FusedStream,
    C: Default + Extend<St::Ok>,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

impl<St, C> Future for TryCollect<St, C>
where
    St: TryStream,
    C: Default + Extend<St::Ok>,
{
    type Output = Result<C, St::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };

        loop {
            match stream.as_mut().try_poll_next(cx) {
                Poll::Ready(Some(Ok(item))) => {
                    this.items.extend(Some(item));
                }
                Poll::Ready(Some(Err(e))) => break Poll::Ready(Err(e)),
                Poll::Ready(None) => break Poll::Ready(Ok(mem::take(&mut this.items))),
                Poll::Pending => break Poll::Pending,
            }
        }
    }
}
