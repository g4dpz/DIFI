// SPDX-License-Identifier: Apache-2.0
//! UDP transport for DIFI packets (spec task 9 / Req 5).
//!
//! The transport is deliberately decoupled from the packet model: it moves opaque
//! byte buffers, each buffer being exactly one DIFI packet (DIFI is datagram-oriented,
//! one packet per UDP payload). Encode/decode never touch sockets, so the codec stays
//! unit-testable without the network.
//!
//! - [`UdpSender`] sends pre-encoded packet bytes to a destination.
//! - [`UdpReceiver`] receives packets and can filter by Stream ID (Req 5.3), so
//!   multiple streams sharing one port are separable.
//!
//! A **TCP** dev-convenience mode ([`TcpSender`]/[`TcpReceiver`], Req 5.2) is also
//! provided for lossless file-like exchange during development. TCP is a byte stream
//! with no datagram boundaries, so each DIFI packet is **length-framed** with a 4-byte
//! big-endian length prefix. This framing is NOT part of the DIFI standard (which is
//! UDP-only); it is a development convenience and will not interoperate with a
//! standard DIFI UDP endpoint.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use crate::constants::{HEADER_BYTES, MAX_PAYLOAD_OCTETS};

/// Reads the Stream ID (word 1) out of a raw DIFI packet without full decoding.
/// Returns `None` if the buffer is too short to contain a header.
pub fn peek_stream_id(buf: &[u8]) -> Option<u32> {
    if buf.len() < HEADER_BYTES {
        return None;
    }
    Some(u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]))
}

/// Sends pre-encoded DIFI packets over UDP.
pub struct UdpSender {
    socket: UdpSocket,
    dest: SocketAddr,
}

impl UdpSender {
    /// Bind an ephemeral local port and target `dest`.
    pub fn new<A: ToSocketAddrs>(dest: A) -> io::Result<Self> {
        let dest = dest
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no dest addr"))?;
        let bind: SocketAddr = if dest.is_ipv4() {
            "0.0.0.0:0".parse().unwrap()
        } else {
            "[::]:0".parse().unwrap()
        };
        let socket = UdpSocket::bind(bind)?;
        Ok(Self { socket, dest })
    }

    /// Send one encoded DIFI packet. Returns the number of bytes sent.
    pub fn send(&self, packet: &[u8]) -> io::Result<usize> {
        self.socket.send_to(packet, self.dest)
    }
}

/// Receives DIFI packets over UDP, optionally filtering by Stream ID.
pub struct UdpReceiver {
    socket: UdpSocket,
    stream_filter: Option<u32>,
    buf: Vec<u8>,
}

impl UdpReceiver {
    /// Bind to `addr` (e.g. `"127.0.0.1:4991"` or `"0.0.0.0:0"` for an ephemeral port).
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        // Room for the largest DIFI payload plus slack.
        let buf = vec![0u8; MAX_PAYLOAD_OCTETS + 64];
        Ok(Self {
            socket,
            stream_filter: None,
            buf,
        })
    }

    /// The actual local address bound (useful when binding to port 0).
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Only deliver packets whose Stream ID matches `stream_id` (Req 5.3).
    pub fn set_stream_filter(&mut self, stream_id: Option<u32>) {
        self.stream_filter = stream_id;
    }

    /// Set a receive timeout; `None` blocks indefinitely.
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.socket.set_read_timeout(dur)
    }

    /// Receive the next packet that passes the Stream-ID filter.
    ///
    /// Packets not matching the filter are silently dropped (they belong to another
    /// stream on the shared port). Returns the packet bytes and the sender address.
    pub fn recv(&mut self) -> io::Result<(Vec<u8>, SocketAddr)> {
        loop {
            let (n, from) = self.socket.recv_from(&mut self.buf)?;
            let packet = &self.buf[..n];
            match self.stream_filter {
                Some(want) => {
                    if peek_stream_id(packet) == Some(want) {
                        return Ok((packet.to_vec(), from));
                    }
                    // else: not our stream, keep receiving.
                }
                None => return Ok((packet.to_vec(), from)),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TCP dev-mode transport (Req 5.2) — NON-STANDARD, length-framed.
// ---------------------------------------------------------------------------

/// Maximum frame length accepted by the TCP receiver (guards against a bad/hostile
/// length prefix). One DIFI packet never exceeds the max payload.
const TCP_MAX_FRAME: usize = MAX_PAYLOAD_OCTETS + 64;

/// Sends DIFI packets over a TCP connection, length-framed (4-byte big-endian prefix).
///
/// **Non-standard**: DIFI's transport is UDP. This is a development convenience for
/// lossless, ordered exchange (e.g. piping a capture between two processes).
pub struct TcpSender {
    stream: TcpStream,
}

impl TcpSender {
    /// Connect to a TCP `dest` (a [`TcpReceiver`] must be listening there).
    pub fn connect<A: ToSocketAddrs>(dest: A) -> io::Result<Self> {
        let stream = TcpStream::connect(dest)?;
        stream.set_nodelay(true).ok();
        Ok(Self { stream })
    }

    /// Send one encoded DIFI packet, length-framed. Returns bytes written (incl. frame).
    pub fn send(&mut self, packet: &[u8]) -> io::Result<usize> {
        if packet.len() > TCP_MAX_FRAME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "packet exceeds TCP frame limit",
            ));
        }
        let len = packet.len() as u32;
        self.stream.write_all(&len.to_be_bytes())?;
        self.stream.write_all(packet)?;
        self.stream.flush()?;
        Ok(packet.len() + 4)
    }
}

/// Receives length-framed DIFI packets over a single TCP connection.
///
/// **Non-standard** (see [`TcpSender`]). Optional Stream-ID filtering mirrors the UDP
/// receiver (Req 5.3).
pub struct TcpReceiver {
    listener: TcpListener,
    conn: Option<TcpStream>,
    stream_filter: Option<u32>,
}

impl TcpReceiver {
    /// Bind and listen on `addr` (e.g. `"127.0.0.1:0"` for an ephemeral port).
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        Ok(Self {
            listener,
            conn: None,
            stream_filter: None,
        })
    }

    /// The actual local address bound (useful when binding to port 0).
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Only deliver packets whose Stream ID matches `stream_id` (Req 5.3).
    pub fn set_stream_filter(&mut self, stream_id: Option<u32>) {
        self.stream_filter = stream_id;
    }

    fn ensure_conn(&mut self) -> io::Result<()> {
        if self.conn.is_none() {
            let (stream, _peer) = self.listener.accept()?;
            stream.set_nodelay(true).ok();
            self.conn = Some(stream);
        }
        Ok(())
    }

    /// Receive the next packet that passes the Stream-ID filter. Accepts a single
    /// connection on first call. Returns `Ok(None)` at clean end-of-stream.
    pub fn recv(&mut self) -> io::Result<Option<Vec<u8>>> {
        self.ensure_conn()?;
        let stream = self.conn.as_mut().expect("conn set");
        loop {
            let mut len_buf = [0u8; 4];
            match read_exact_or_eof(stream, &mut len_buf)? {
                false => return Ok(None), // clean EOF at a frame boundary
                true => {}
            }
            let len = u32::from_be_bytes(len_buf) as usize;
            if len > TCP_MAX_FRAME {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "TCP frame length exceeds limit",
                ));
            }
            let mut packet = vec![0u8; len];
            stream.read_exact(&mut packet)?;
            match self.stream_filter {
                Some(want) => {
                    if peek_stream_id(&packet) == Some(want) {
                        return Ok(Some(packet));
                    }
                    // else: not our stream, keep reading.
                }
                None => return Ok(Some(packet)),
            }
        }
    }
}

/// Read exactly `buf.len()` bytes. Returns Ok(false) if EOF occurs before any byte is
/// read (clean end of stream); errors if EOF occurs mid-frame.
fn read_exact_or_eof(stream: &mut TcpStream, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match stream.read(&mut buf[filled..]) {
            Ok(0) => {
                if filled == 0 {
                    return Ok(false); // clean EOF at frame boundary
                }
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "EOF in the middle of a TCP frame",
                ));
            }
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}
