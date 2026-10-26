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
//! TCP is intentionally not implemented here: DIFI's standard transport is UDP
//! (Req 5.1). A TCP dev-convenience mode (Req 5.2) can be added later, clearly marked
//! non-standard.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
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
