//! A fixed connection budget surrounds axum's connection tasks. Slow clients
//! retain one permit, preventing an unbounded live task/connection collection.
use std::{
    io,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
};
pub(crate) struct ChatListener {
    pub listener: TcpListener,
    pub capacity: Arc<Semaphore>,
}
pub(crate) struct ChatIo {
    stream: TcpStream,
    _permit: OwnedSemaphorePermit,
}
impl axum::serve::Listener for ChatListener {
    type Io = ChatIo;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            // This semaphore is private and never closed.
            let permit = self
                .capacity
                .clone()
                .acquire_owned()
                .await
                .expect("private connection semaphore");
            match self.listener.accept().await {
                Ok((stream, address)) => {
                    return (
                        ChatIo {
                            stream,
                            _permit: permit,
                        },
                        address,
                    );
                }
                Err(_) => {
                    drop(permit);
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}
impl AsyncRead for ChatIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}
impl AsyncWrite for ChatIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
