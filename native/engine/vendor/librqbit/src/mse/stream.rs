//! Transparent RC4 stream wrappers for the post-handshake BitTorrent payload.
//!
//! The two directions use independent RC4 states (MSE derives a distinct key
//! for the encrypt and decrypt directions). The wrappers implement
//! [`tokio::io::AsyncRead`] / [`tokio::io::AsyncWrite`] so that librqbit's
//! existing `ReadBuf` and `write_all` paths work unchanged once the handshake
//! has swapped in these streams.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::rc4::Rc4;

fn write_zero_err() -> io::Error {
    io::Error::new(io::ErrorKind::WriteZero, "underlying stream returned Ok(0)")
}

/// Decrypting wrapper around an inbound stream.
pub struct Rc4Reader<R> {
    inner: R,
    rc4: Rc4,
}

impl<R> Rc4Reader<R> {
    pub fn new(inner: R, rc4: Rc4) -> Self {
        Rc4Reader { inner, rc4 }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Rc4Reader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let poll = Pin::new(&mut this.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = poll {
            let newly = buf.filled_mut();
            this.rc4.apply_keystream(&mut newly[before..]);
        }
        poll
    }
}

/// Encrypting wrapper around an outbound stream.
pub struct Rc4Writer<W> {
    inner: W,
    rc4: Rc4,
    /// Encrypted bytes that have been consumed from the caller's buffer but
    /// not yet flushed to `inner`.
    pending: Vec<u8>,
}

impl<W> Rc4Writer<W> {
    pub fn new(inner: W, rc4: Rc4) -> Self {
        Rc4Writer {
            inner,
            rc4,
            pending: Vec::new(),
        }
    }
}

impl<W: AsyncWrite + Unpin> Rc4Writer<W> {
    /// Attempt to drain `pending` into `inner`. Returns the number of bytes
    /// written so far (the caller applies `drain(..n)` itself).
    fn poll_drain_pending(
        mut inner: Pin<&mut W>,
        pending: &[u8],
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<usize>> {
        let mut written = 0;
        while written < pending.len() {
            match inner.as_mut().poll_write(cx, &pending[written..]) {
                Poll::Ready(Ok(0)) => return Poll::Ready(Err(write_zero_err())),
                Poll::Ready(Ok(n)) => written += n,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => break,
            }
        }
        Poll::Ready(Ok(written))
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for Rc4Writer<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();

        // If we still hold encrypted bytes from a previous partial write, those
        // bytes *are* the ciphertext of the `data` the caller is re-submitting
        // (tokio's `write_all` re-sends `data[n..]` after a short write). Flush
        // them first without re-encrypting `data`.
        if !this.pending.is_empty() {
            let written =
                match Self::poll_drain_pending(Pin::new(&mut this.inner), &this.pending, cx) {
                    Poll::Ready(Ok(n)) => n,
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                };
            this.pending.drain(..written);
            if !this.pending.is_empty() {
                return Poll::Pending;
            }
            return Poll::Ready(Ok(data.len()));
        }

        // No pending ciphertext: encrypt the whole buffer and write it out.
        let mut encrypted = data.to_vec();
        this.rc4.apply_keystream(&mut encrypted);

        let mut written = 0;
        while written < encrypted.len() {
            match Pin::new(&mut this.inner).poll_write(cx, &encrypted[written..]) {
                Poll::Ready(Ok(0)) => return Poll::Ready(Err(write_zero_err())),
                Poll::Ready(Ok(n)) => written += n,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => break,
            }
        }

        if written < encrypted.len() {
            this.pending.extend_from_slice(&encrypted[written..]);
            if written == 0 {
                return Poll::Pending;
            }
        }
        Poll::Ready(Ok(written))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let mut written = 0;
        while written < this.pending.len() {
            match Pin::new(&mut this.inner).poll_write(cx, &this.pending[written..]) {
                Poll::Ready(Ok(0)) => return Poll::Ready(Err(write_zero_err())),
                Poll::Ready(Ok(n)) => written += n,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => {
                    this.pending.drain(..written);
                    return Poll::Pending;
                }
            }
        }
        this.pending.drain(..written);
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
