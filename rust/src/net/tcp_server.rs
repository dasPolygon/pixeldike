use crate::net::framing::Frame;
use crate::pixmap::SharedPixmap;
use bytes::{Buf, BytesMut};
use std::io::Cursor;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::io::*;

pub async fn start(pixmap: SharedPixmap) {
    let listener = TcpListener::bind("0.0.0.0:1234").await.unwrap();
    info!(target: "TCP", "Started server on {}", listener.local_addr().unwrap());

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let pixmap = pixmap.clone();
        tokio::spawn(async move {
            process_connection(TcpConnection::new(socket), pixmap).await;
        });
    }
}

async fn process_connection(mut connection: TcpConnection, pixmap: SharedPixmap) {
    debug!(target: "TCP", "Client connected {}", connection.peer_address);
    loop {
        // receive a frame from the client with regards to the client closing the connection
        let frame = match connection.read_frame().await {
            Err(e) => {
                warn!(target: "TCP", "Error reading frame {}", e);
                return;
            }
            Ok(opt) => match opt {
                None => {
                    debug!(target: "TCP", "Client disconnected: {}", connection.peer_address);
                    return;
                }
                Some(frame) => frame,
            },
        };

        // handle the frame
        let response = super::handle_frame(frame, &pixmap);

        // send the response back to the client (if there is one)
        match response {
            None => {}
            Some(response) => match connection.write_frame(response).await {
                Err(e) => {
                    warn!(target: "TCP", "Error writing frame {}", e);
                    return;
                }
                _ => {}
            },
        }
    }
}

pub(crate) struct TcpConnection {
    stream: BufWriter<TcpStream>,
    buffer: BytesMut,
    peer_address: SocketAddr,
}

impl TcpConnection {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            peer_address: stream.peer_addr().unwrap(),
            stream: BufWriter::new(stream),
            buffer: BytesMut::with_capacity(4096),
        }
    }

    pub(self) async fn read_frame(&mut self) -> Result<Option<Frame>> {
        loop {
            // Attempt to read more data from the socket.
            //
            // On success, the number of bytes is returned.
            // `0` indicates `end of stream`
            if self.stream.read_buf(&mut self.buffer).await? == 0 {
                // The remote closed the connection.
                // For this to be a clean shutdown, there should be no data in the buffer.
                // If there is, this means that the peer closed the socket while sending a frame.
                return if self.buffer.is_empty() {
                    Ok(None)
                } else {
                    Err(std::io::ErrorKind::ConnectionReset.into())
                };
            }

            // Attempt to parse a frame from the buffered data.
            // If enough data has been buffered, the frame is returned.
            if let Some(frame) = self.parse_frame() {
                return Ok(Some(frame));
            }
        }
    }

    pub(self) async fn write_frame(&mut self, frame: Frame) -> Result<()> {
        self.stream.write_all(&frame.encode()).await?;
        self.stream.flush().await?;

        Ok(())
    }

    fn parse_frame(&mut self) -> Option<Frame> {
        let mut buf = Cursor::new(&self.buffer[..]);

        // Check whether a full frame is available
        match Frame::check(&mut buf) {
            Err(_) => None,
            Ok(_) => {
                // Retrieve to where `check` has read the buffer
                let len = buf.position() as usize;
                // Reset the cursor so that `parse` can read the same bytes
                buf.set_position(0);

                let frame = Frame::parse(&mut buf).ok().unwrap();
                self.buffer.advance(len);
                Some(frame)
            }
        }
    }
}
