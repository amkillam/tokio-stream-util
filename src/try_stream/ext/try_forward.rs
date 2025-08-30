use super::into_stream::IntoFuseStream;
use super::TryStream;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_core::future::{FusedFuture, Future};
use tokio_sink::Sink;
use tokio_stream::Stream;

/// Future for the [`try_forward`](super::TryStreamExt::try_forward) method.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct TryForward<St, Si>
where
    St: TryStream,
{
    sink: Option<Si>,
    stream: IntoFuseStream<St>,
    buffered_item: Option<St::Ok>,
}

impl<St, Si> Unpin for TryForward<St, Si>
where
    St: TryStream + Unpin,
    Si: Unpin,
{
}

impl<St, Si> TryForward<St, Si>
where
    St: TryStream,
{
    pub(super) fn new(stream: St, sink: Si) -> Self {
        Self {
            sink: Some(sink),
            stream: IntoFuseStream::new(stream),
            buffered_item: None,
        }
    }
}

impl<St, Si> FusedFuture for TryForward<St, Si>
where
    St: TryStream,
    Si: Sink<St::Ok, Error = St::Error>,
{
    fn is_terminated(&self) -> bool {
        self.sink.is_none()
    }
}

impl<St, Si> Future for TryForward<St, Si>
where
    St: TryStream,
    Si: Sink<St::Ok, Error = St::Error>,
{
    type Output = Result<(), St::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut sink_opt_pin = unsafe { Pin::new_unchecked(&mut this.sink) };

        loop {
            let mut sink = match sink_opt_pin.as_mut().as_pin_mut() {
                Some(s) => s,
                None => panic!("polled `TryForward` after completion"),
            };

            // If we've got an item buffered already, we need to write it to the
            // sink before we can do anything else
            if this.buffered_item.is_some() {
                match sink.as_mut().poll_ready(cx) {
                    Poll::Ready(Ok(())) => {
                        let item = this.buffered_item.take().unwrap();
                        if let Err(e) = sink.as_mut().start_send(item) {
                            return Poll::Ready(Err(e));
                        }
                    }
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                }
            }

            let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };
            match stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(item))) => {
                    this.buffered_item = Some(item);
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(e));
                }
                Poll::Ready(None) => match sink.as_mut().poll_close(cx) {
                    Poll::Ready(Ok(())) => {
                        sink_opt_pin.set(None);
                        return Poll::Ready(Ok(()));
                    }
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                },
                Poll::Pending => {
                    match sink.as_mut().poll_flush(cx) {
                        Poll::Ready(Ok(())) => {}
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => return Poll::Pending,
                    }
                    return Poll::Pending;
                }
            }
        }
    }
}
