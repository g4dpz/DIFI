// SPDX-License-Identifier: Apache-2.0
//! DIFI Encoder — builds valid packets and maintains the mod-16 stream sequence number.

use crate::constants::*;
use crate::error::Result;
use crate::iq::IqSample;
use crate::packet::{
    DataFormat, Header, SignalContextPacket, SignalDataPacket, VersionContextPacket,
};

/// Stateful encoder for one DIFI stream (one Stream ID).
///
/// Maintains the mod-16 sequence number across successive Signal Data packets
/// (Req 2.3). Context/version packets carry their own sequence per the standard; this
/// encoder increments the shared stream sequence for data packets only.
pub struct StreamEncoder {
    pub stream_id: u32,
    pub bit_depth: u8,
    seq: u8,
}

impl StreamEncoder {
    pub fn new(stream_id: u32, bit_depth: u8) -> Self {
        Self {
            stream_id,
            bit_depth,
            seq: 0,
        }
    }

    /// The sequence number that will be used by the next `encode_data` call.
    pub fn next_seq(&self) -> u8 {
        self.seq
    }

    #[allow(clippy::too_many_arguments)]
    fn header(
        &self,
        packet_type: u8,
        tsm_bit: u8,
        information_class: u16,
        packet_class: u16,
        seq: u8,
        int_s: u32,
        frac_ps: u64,
    ) -> Header {
        Header {
            packet_type,
            class_id_present: true,
            tsm: tsm_bit,
            tsi: tsi::UTC,
            tsf: tsf::REAL_TIME_PS,
            seq_num: seq & 0xF,
            stream_id: self.stream_id,
            information_class,
            packet_class,
            integer_seconds: int_s,
            fractional_seconds: frac_ps,
        }
    }

    /// Encode a Signal Data packet and advance the mod-16 sequence number.
    pub fn encode_data(
        &mut self,
        samples: &[IqSample],
        integer_seconds: u32,
        fractional_ps: u64,
    ) -> Result<Vec<u8>> {
        let header = self.header(
            packet_type::SIGNAL_DATA,
            tsm::DATA,
            information_class::STANDARD_FLOW,
            packet_class::SIGNAL_DATA,
            self.seq,
            integer_seconds,
            fractional_ps,
        );
        let pkt = SignalDataPacket {
            header,
            bit_depth: self.bit_depth,
            samples: samples.to_vec(),
        };
        let bytes = pkt.pack()?;
        self.seq = (self.seq + 1) & 0xF;
        Ok(bytes)
    }

    /// Encode a DIFI 1.2 **Sample Count** Signal Data packet (`0x0002`): identical layout
    /// to [`encode_data`], but Packet Class `0x0002`, Information Class
    /// "Data Plane plus Flow Control", and TSF = sample count, with `fractional`
    /// carrying a sample count rather than picoseconds. Advances the mod-16 sequence.
    pub fn encode_data_sample_count(
        &mut self,
        samples: &[IqSample],
        integer_seconds: u32,
        fractional_sample_count: u64,
    ) -> Result<Vec<u8>> {
        let mut header = self.header(
            packet_type::SIGNAL_DATA,
            tsm::DATA,
            information_class::DATA_PLANE_FLOW_CONTROL,
            packet_class::SAMPLE_COUNT_SIGNAL_DATA,
            self.seq,
            integer_seconds,
            fractional_sample_count,
        );
        header.tsf = tsf::SAMPLE_COUNT;
        let pkt = SignalDataPacket {
            header,
            bit_depth: self.bit_depth,
            samples: samples.to_vec(),
        };
        let bytes = pkt.pack()?;
        self.seq = (self.seq + 1) & 0xF;
        Ok(bytes)
    }

    /// Largest number of complex samples that fit in one Signal Data packet at this
    /// bit depth, respecting the DIFI max payload (Req 2.5) and 32-bit-word alignment.
    ///
    /// The payload must be a whole number of 32-bit words: `samples * 2 * bit_depth`
    /// must be a multiple of 32, i.e. `samples` is a multiple of `lcm(16, bit_depth)
    /// / bit_depth`. This returns the largest aligned sample count whose packet
    /// (header + payload) stays within `MAX_PAYLOAD_OCTETS`.
    pub fn max_samples_per_packet(&self) -> usize {
        let bd = self.bit_depth as usize;
        // Samples per 32-bit-word-aligned block.
        let step = lcm(16, bd) / bd; // smallest sample count that is word aligned
        let payload_octets = MAX_PAYLOAD_OCTETS - HEADER_BYTES;
        let payload_bits = payload_octets * 8;
        let max_samples_unaligned = payload_bits / (2 * bd);
        // Round down to a multiple of `step`.
        (max_samples_unaligned / step) * step
    }

    /// Encode a (possibly large) sample buffer as a sequence of Signal Data packets,
    /// segmenting at the DIFI max-payload boundary (Req 2.5: "reject OR segment").
    /// Each packet carries the running mod-16 sequence number. Returns one byte buffer
    /// per packet.
    pub fn encode_data_segmented(
        &mut self,
        samples: &[IqSample],
        integer_seconds: u32,
        fractional_ps: u64,
    ) -> Result<Vec<Vec<u8>>> {
        let chunk = self.max_samples_per_packet();
        if chunk == 0 {
            return Err(crate::error::DifiError::Invalid {
                field: "bit_depth",
                detail: "no whole-word sample chunk fits the max payload".into(),
            });
        }
        let mut out = Vec::new();
        if samples.is_empty() {
            return Ok(out);
        }
        for part in samples.chunks(chunk) {
            out.push(self.encode_data(part, integer_seconds, fractional_ps)?);
        }
        Ok(out)
    }

    /// Encode a conformant Signal Context packet describing this stream.
    #[allow(clippy::too_many_arguments)]
    pub fn encode_context(
        &self,
        rf_reference_hz: f64,
        bandwidth_hz: f64,
        sample_rate_hz: f64,
        gain_db: f64,
        integer_seconds: u32,
        fractional_ps: u64,
    ) -> Result<Vec<u8>> {
        let header = self.header(
            packet_type::CONTEXT,
            tsm::CONTEXT,
            information_class::STANDARD_FLOW,
            packet_class::SIGNAL_CONTEXT,
            0,
            integer_seconds,
            fractional_ps,
        );
        SignalContextPacket {
            header,
            context_changed: true,
            rf_reference_hz,
            if_reference_hz: 0.0,
            bandwidth_hz,
            sample_rate_hz,
            gain_db,
            data_format: DataFormat {
                bit_depth: self.bit_depth,
            },
        }
        .pack()
    }

    /// Encode a DIFI 1.2 **Sample Count** Signal Context packet (`0x0003`): identical
    /// layout to [`encode_context`], but Packet Class `0x0003`, Information Class
    /// "Data Plane plus Flow Control", and TSF = sample count.
    #[allow(clippy::too_many_arguments)]
    pub fn encode_context_sample_count(
        &self,
        rf_reference_hz: f64,
        bandwidth_hz: f64,
        sample_rate_hz: f64,
        gain_db: f64,
        integer_seconds: u32,
        fractional_sample_count: u64,
    ) -> Result<Vec<u8>> {
        let mut header = self.header(
            packet_type::CONTEXT,
            // Per the v1.2.1 reference, Packet Class 0x0003 requires TSM=0 (fine), unlike
            // the standard-flow context (0x0001) which uses TSM=1.
            tsm::DATA,
            information_class::DATA_PLANE_FLOW_CONTROL,
            packet_class::SAMPLE_COUNT_SIGNAL_CONTEXT,
            0,
            integer_seconds,
            fractional_sample_count,
        );
        header.tsf = tsf::SAMPLE_COUNT;
        SignalContextPacket {
            header,
            context_changed: true,
            rf_reference_hz,
            if_reference_hz: 0.0,
            bandwidth_hz,
            sample_rate_hz,
            gain_db,
            data_format: DataFormat {
                bit_depth: self.bit_depth,
            },
        }
        .pack()
    }

    /// Encode a conformant Version Context packet.
    pub fn encode_version(
        &self,
        year: u16,
        day: u16,
        integer_seconds: u32,
        fractional_ps: u64,
    ) -> Result<Vec<u8>> {
        let header = self.header(
            version_packet_type::VERSION_CONTEXT,
            tsm::CONTEXT,
            information_class::VERSION_FLOW,
            packet_class::VERSION_CONTEXT,
            0,
            integer_seconds,
            fractional_ps,
        );
        VersionContextPacket {
            header,
            context_changed: true,
            year,
            day,
        }
        .pack()
    }
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

fn lcm(a: usize, b: usize) -> usize {
    a / gcd(a, b) * b
}
