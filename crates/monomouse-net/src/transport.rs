use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info};
use crate::protocol::Message;

/// A framed connection that sends/receives length-prefixed JSON messages.
pub struct Connection {
    stream: TcpStream,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        stream.set_nodelay(true).ok();
        Self { stream }
    }

    pub async fn connect(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr).await.context("failed to connect")?;
        stream.set_nodelay(true).ok();
        info!("Connected to {addr}");
        Ok(Self { stream })
    }

    /// Split into separate reader and writer halves for concurrent use.
    pub fn split(self) -> (ConnectionReader, ConnectionWriter) {
        let (read, write) = tokio::io::split(self.stream);
        (ConnectionReader { reader: read }, ConnectionWriter { writer: write })
    }

    /// Send a message as length-prefixed JSON.
    pub async fn send(&mut self, msg: &Message) -> Result<()> {
        send_message(&mut self.stream, msg).await
    }

    /// Receive a length-prefixed JSON message.
    pub async fn recv(&mut self) -> Result<Message> {
        recv_message(&mut self.stream).await
    }
}

/// Read half of a split connection.
pub struct ConnectionReader {
    reader: ReadHalf<TcpStream>,
}

impl ConnectionReader {
    pub async fn recv(&mut self) -> Result<Message> {
        recv_message(&mut self.reader).await
    }
}

/// Write half of a split connection.
pub struct ConnectionWriter {
    writer: WriteHalf<TcpStream>,
}

impl ConnectionWriter {
    pub async fn send(&mut self, msg: &Message) -> Result<()> {
        send_message(&mut self.writer, msg).await
    }
}

async fn send_message<W: AsyncWriteExt + Unpin>(writer: &mut W, msg: &Message) -> Result<()> {
    let json = serde_json::to_vec(msg)?;
    let len = json.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&json).await?;
    writer.flush().await?;
    debug!("Sent message ({len} bytes)");
    Ok(())
}

async fn recv_message<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<Message> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await.context("connection closed")?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 1_048_576 {
        anyhow::bail!("message too large: {len} bytes");
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    let msg: Message = serde_json::from_slice(&buf)?;
    debug!("Received message ({len} bytes)");
    Ok(msg)
}

/// TCP listener wrapper.
pub struct Listener {
    inner: TcpListener,
}

impl Listener {
    pub async fn bind(addr: &str) -> Result<Self> {
        let inner = TcpListener::bind(addr).await.context("failed to bind")?;
        info!("Listening on {addr}");
        Ok(Self { inner })
    }

    pub async fn accept(&self) -> Result<(Connection, std::net::SocketAddr)> {
        let (stream, addr) = self.inner.accept().await?;
        info!("Accepted connection from {addr}");
        Ok((Connection::new(stream), addr))
    }
}
