//! DIFI Encoder — builds valid packets and maintains the mod-16 stream sequence number.

use crate::constants::*;
use crate::error::Result;
use crate::iq::IqSample;
use crate::packet::{DataFormat, Header, SignalContextPacket, SignalDataPacket, VersionContextPacket};

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
