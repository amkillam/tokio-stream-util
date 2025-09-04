use super::TryStream;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::cmp;
use std::io::{Error, Result};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, ReadBuf};

/// Reader for the [`into_async_read`](super::TryStreamExt::into_async_read) method.
#[derive(Debug)]
#[must_use = "readers do nothing unless polled"]
pub struct IntoAsyncRead<St>
where
    St: TryStream<Error = Error>,
    St::Ok: AsRef<[u8]>,
{
    stream: St,
    state: ReadState<St::Ok>,
}

#[derive(Debug)]
enum ReadState<T: AsRef<[u8]>> {
    Ready { chunk: T, chunk_start: usize },
    PendingChunk,
    Eof,
}

impl<St> IntoAsyncRead<St>
where
    St: TryStream<Error = Error>,
    St::Ok: AsRef<[u8]>,
{
    pub(super) fn new(stream: St) -> Self {
        Self {
            stream,
            state: ReadState::PendingChunk,
        }
    }
}

impl<St> AsyncRead for IntoAsyncRead<St>
where
    St: TryStream<Error = Error>,
    St::Ok: AsRef<[u8]>,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };

        loop {
            match &mut this.state {
                ReadState::Ready { chunk, chunk_start } => {
                    let chunk_slice = chunk.as_ref();
                    let available = &chunk_slice[*chunk_start..];
                    let len = cmp::min(buf.remaining(), available.len());

                    buf.put_slice(&available[..len]);
                    *chunk_start += len;

                    if chunk_slice.len() == *chunk_start {
                        this.state = ReadState::PendingChunk;
                    }

                    return Poll::Ready(Ok(()));
                }
                ReadState::PendingChunk => {
                    let result = match stream.as_mut().try_poll_next(cx) {
                        Poll::Ready(result) => result,
                        Poll::Pending => return Poll::Pending,
                    };
                    match result {
                        Some(Ok(chunk)) => {
                            if !chunk.as_ref().is_empty() {
                                this.state = ReadState::Ready {
                                    chunk,
                                    chunk_start: 0,
                                };
                            }
                        }
                        Some(Err(err)) => {
                            this.state = ReadState::Eof;
                            return Poll::Ready(Err(err));
                        }
                        None => {
                            this.state = ReadState::Eof;
                            return Poll::Ready(Ok(()));
                        }
                    }
                }
                ReadState::Eof => {
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl<St> AsyncWrite for IntoAsyncRead<St>
where
    St: TryStream<Error = Error> + AsyncWrite,
    St::Ok: AsRef<[u8]>,
{
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize>> {
        let this = unsafe { self.get_unchecked_mut() };
        unsafe { Pin::new_unchecked(&mut this.stream) }.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        let this = unsafe { self.get_unchecked_mut() };
        unsafe { Pin::new_unchecked(&mut this.stream) }.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        let this = unsafe { self.get_unchecked_mut() };
        unsafe { Pin::new_unchecked(&mut this.stream) }.poll_shutdown(cx)
    }
}

impl<St> AsyncBufRead for IntoAsyncRead<St>
where
    St: TryStream<Error = Error>,
    St::Ok: AsRef<[u8]>,
{
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<&[u8]>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut stream = unsafe { Pin::new_unchecked(&mut this.stream) };

        while let ReadState::PendingChunk = &mut this.state {
            match stream.as_mut().try_poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    if !chunk.as_ref().is_empty() {
                        this.state = ReadState::Ready {
                            chunk,
                            chunk_start: 0,
                        };
                    }
                }
                Poll::Ready(Some(Err(err))) => {
                    this.state = ReadState::Eof;
                    return Poll::Ready(Err(err));
                }
                Poll::Ready(None) => {
                    this.state = ReadState::Eof;
                    return Poll::Ready(Ok(&[]));
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        match &this.state {
            ReadState::Ready { chunk, chunk_start } => {
                let chunk = chunk.as_ref();
                Poll::Ready(Ok(&chunk[*chunk_start..]))
            }
            ReadState::Eof => Poll::Ready(Ok(&[])),
            ReadState::PendingChunk => unreachable!(),
        }
    }

    fn consume(self: Pin<&mut Self>, amount: usize) {
        let this = unsafe { self.get_unchecked_mut() };

        // https://github.com/rust-lang/futures-rs/pull/1556#discussion_r281644295
        if amount == 0 {
            return;
        }
        if let ReadState::Ready { chunk, chunk_start } = &mut this.state {
            *chunk_start += amount;
            debug_assert!(*chunk_start <= chunk.as_ref().len());
            if *chunk_start >= chunk.as_ref().len() {
                this.state = ReadState::PendingChunk;
            }
        } else {
            debug_assert!(
                false,
                "Attempted to consume from IntoAsyncRead without chunk"
            );
        }
    }
}
