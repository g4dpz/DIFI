// SPDX-License-Identifier: Apache-2.0
//! DIFI Decoder — classifies raw bytes into the packet model, fail-closed.
//!
//! A real receiver learns the sample bit depth from a Signal Context packet before it
//! can decode Signal Data. [`Decoder`] tracks the last seen bit depth per Stream ID so
//! a stream can be decoded in arrival order.

use std::collections::HashMap;

use crate::constants::*;
use crate::error::{DifiError, Result};
use crate::flowcontrol::TimingFlowControlPacket;
use crate::packet::{SignalContextPacket, SignalDataPacket, VersionContextPacket};

/// A decoded DIFI packet: the three DIFI 1.1 kinds plus the DIFI 1.2 Timing Flow Control.
#[derive(Debug, Clone, PartialEq)]
pub enum DifiPacket {
    SignalData(SignalDataPacket),
    SignalContext(SignalContextPacket),
    VersionContext(VersionContextPacket),
    /// DIFI 1.2 Timing Flow Control (Packet Class `0x0005`/`0x0006`).
    TimingFlowControl(TimingFlowControlPacket),
}

/// Peek the VRT packet type and (for context packets) packet class from a buffer.
/// Fails closed on truncation or bad OUI before any field is trusted.
fn classify(buf: &[u8]) -> Result<(u8, u16, u32)> {
    if buf.len() < HEADER_BYTES {
        return Err(DifiError::Truncated {
            field: "header",
            need: HEADER_BYTES,
            have: buf.len(),
        });
    }
    let word0 = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let packet_type = ((word0 >> 28) & 0xF) as u8;
    let oui =
        u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]) & 0x00FF_FFFF;
    if oui != (VITA_OUI & 0x00FF_FFFF) {
        return Err(DifiError::BadOui {
            expected: VITA_OUI & 0x00FF_FFFF,
            actual: oui,
        });
    }
    let word3 = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let packet_class = (word3 & 0xFFFF) as u16;
    let stream_id = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    Ok((packet_type, packet_class, stream_id))
}

/// Stateful decoder that remembers each stream's sample format from context packets.
#[derive(Default)]
pub struct Decoder {
    bit_depth_by_stream: HashMap<u32, u8>,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Supply a bit depth for a stream without having seen a context packet
    /// (useful for golden-fixture decoding where the format is known out of band).
    pub fn set_bit_depth(&mut self, stream_id: u32, bit_depth: u8) {
        self.bit_depth_by_stream.insert(stream_id, bit_depth);
    }

    /// Decode a single packet, updating stream state from context packets.
    pub fn decode(&mut self, buf: &[u8]) -> Result<DifiPacket> {
        let (packet_type, packet_class, stream_id) = classify(buf)?;
        match packet_type {
            // Signal Context: VRT type 0x4 with the signal-context class.
            t if t == packet_type::CONTEXT && packet_class == packet_class::SIGNAL_CONTEXT => {
                let ctx = SignalContextPacket::unpack(buf)?;
                self.bit_depth_by_stream
                    .insert(ctx.header.stream_id, ctx.data_format.bit_depth);
                Ok(DifiPacket::SignalContext(ctx))
            }
            // Version Context: VRT type 0x5.
            t if t == version_packet_type::VERSION_CONTEXT => {
                Ok(DifiPacket::VersionContext(VersionContextPacket::unpack(buf)?))
            }
            // Timing Flow Control: VRT type 0x6 (Command). Decoded BEFORE the Signal Data
            // bit-depth gate below — flow-control packets carry no sample bit depth and
            // must decode regardless of whether a Signal Context has been seen for the
            // stream (spec difi-1.2-dataplane task 3 note).
            t if t == packet_type::COMMAND => Ok(DifiPacket::TimingFlowControl(
                TimingFlowControlPacket::unpack(buf)?,
            )),
            t if t == packet_type::SIGNAL_DATA => {
                let bit_depth = self.bit_depth_by_stream.get(&stream_id).copied().ok_or(
                    DifiError::Invalid {
                        field: "bit_depth",
                        detail: format!(
                            "no Signal Context seen for stream 0x{stream_id:08X}; cannot decode data"
                        ),
                    },
                )?;
                Ok(DifiPacket::SignalData(SignalDataPacket::unpack(buf, bit_depth)?))
            }
            other => Err(DifiError::BadPacketType { actual: other }),
        }
    }
}
