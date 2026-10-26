// SPDX-License-Identifier: Apache-2.0
//! DIFI 1.2 Timing Flow Control packets (Packet Classes `0x0005`/`0x0006`).
//!
//! A Timing Flow Control packet is a VITA 49.2 Command Packet (VRT type `0x6`) that a
//! Sink uses to convey buffer fill level and timing to a Source, so the Source can pace
//! its stream. It is built on the shared [`crate::command`] core.
//!
//! Wire layout (21 words / 84 bytes), confirmed against the §4.4.2 field descriptions:
//!
//! ```text
//! words 1-7   prologue (Header)
//! word  8     CAM                               (byte 28)
//! word  9     Message ID                        (byte 32)
//! word  10    Controllee ID                     (byte 36)
//! word  11    Controller ID                     (byte 40)
//! word  12    CIF0  (bit30 RefPoint, bit21 SampleRate, bit20 TimestampAdj, bit2 CIF1)
//! word  13    CIF1  (bit1 Buffer Size present)  (byte 48)
//! word  14    Reference Point                   (byte 52)
//! words 15-16 Sample Rate (64-bit)              (byte 56)
//! words 17-18 Timestamp Adjustment (fs, 64-bit) (byte 64)
//! words 19-20 Buffer Size (64-bit)              (byte 72)
//! word  21    Buffer Status (level + OF/NF/NE/UF)  (byte 80)
//! ```
//!
//! The two classes differ only in the TSF interpretation of the prologue timestamp:
//! `0x0005` carries a **sample count**, `0x0006` carries **real-time picoseconds**.

use crate::command::{action_mode, BufferState, Cam, Cif};
use crate::constants::*;
use crate::error::{DifiError, Result};
use crate::packet::{read_u32, read_u64, validate_size, Header};

/// Which Timing Flow Control class a packet is — fixes the Packet Class code and the
/// expected TSF (fractional-timestamp) interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowControlKind {
    /// Packet Class `0x0005` — Sample Count Timing Flow Control (TSF = sample count).
    SampleCount,
    /// Packet Class `0x0006` — Real-Time TSF Timing Flow Control (TSF = real-time ps).
    RealTime,
}

impl FlowControlKind {
    pub fn packet_class(self) -> u16 {
        match self {
            FlowControlKind::SampleCount => packet_class::SAMPLE_COUNT_FLOW_CONTROL,
            FlowControlKind::RealTime => packet_class::REAL_TIME_FLOW_CONTROL,
        }
    }

    /// The TSF value this class uses in the prologue.
    pub fn tsf(self) -> u8 {
        match self {
            FlowControlKind::SampleCount => tsf::SAMPLE_COUNT,
            FlowControlKind::RealTime => tsf::REAL_TIME_PS,
        }
    }

    /// The Information Class paired with this flow-control class, per the v1.2.1
    /// reference: `0x0005` ↔ `0x0002`, `0x0006` ↔ `0x0003`.
    pub fn information_class(self) -> u16 {
        match self {
            FlowControlKind::SampleCount => information_class::DATA_PLANE_FLOW_CONTROL,
            FlowControlKind::RealTime => information_class::DATA_PLANE_FLOW_CONTROL_RT,
        }
    }

    fn from_packet_class(code: u16) -> Option<FlowControlKind> {
        match code {
            c if c == packet_class::SAMPLE_COUNT_FLOW_CONTROL => Some(FlowControlKind::SampleCount),
            c if c == packet_class::REAL_TIME_FLOW_CONTROL => Some(FlowControlKind::RealTime),
            _ => None,
        }
    }
}

/// A DIFI 1.2 Timing Flow Control packet.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingFlowControlPacket {
    pub header: Header,
    pub kind: FlowControlKind,
    pub cam: Cam,
    pub message_id: u32,
    pub controllee_id: u32,
    pub controller_id: u32,
    /// Reference Point ID (default `0x00000064`).
    pub reference_point: u32,
    /// Sample rate in Hz (stored on the wire as integer * 2^20, like the context packet).
    pub sample_rate_hz: f64,
    /// Timestamp adjustment in femtoseconds (signal delay to the Reference Point).
    pub timestamp_adjust_fs: i64,
    /// Buffer capacity + fill-level status.
    pub buffer: BufferState,
}

impl TimingFlowControlPacket {
    pub fn pack(&self) -> Result<Vec<u8>> {
        // Validate the CAM before serialising (fail-closed, not silent-mask): the DIFI
        // command profile requires both IDs present, 32-bit format, and execute mode.
        if !self.cam.controllee_id_present || !self.cam.controller_id_present {
            return Err(DifiError::Invalid {
                field: "cam.id_present",
                detail: "Timing Flow Control requires Controllee and Controller ID present"
                    .into(),
            });
        }
        if self.cam.controllee_id_format || self.cam.controller_id_format {
            return Err(DifiError::Invalid {
                field: "cam.id_format",
                detail: "Timing Flow Control requires 32-bit ID format (format bit 0)".into(),
            });
        }
        if self.cam.action_mode != action_mode::EXECUTE {
            return Err(DifiError::Invalid {
                field: "cam.action_mode",
                detail: format!(
                    "Timing Flow Control requires Action Mode = execute (0b10), was 0b{:02b}",
                    self.cam.action_mode
                ),
            });
        }
        // Header: enforce the class code, Information Class, and TSF for this kind; keep
        // the caller's other prologue fields (TSI, seq, stream id, timestamps). The
        // Information Class is DERIVED from the kind so the invalid pairings (e.g.
        // 0x0002/0x0006) cannot be emitted.
        let mut header = self.header.clone();
        header.packet_type = packet_type::COMMAND;
        header.packet_class = self.kind.packet_class();
        header.information_class = self.kind.information_class();
        header.tsf = self.kind.tsf();

        let total_words = command::WORDS;
        let mut out = Vec::with_capacity(total_words as usize * 4);
        header.write_into(total_words, &mut out);

        // Word 8: CAM.
        out.extend_from_slice(&self.cam.to_word().to_be_bytes());
        // Words 9-11: IDs.
        out.extend_from_slice(&self.message_id.to_be_bytes());
        out.extend_from_slice(&self.controllee_id.to_be_bytes());
        out.extend_from_slice(&self.controller_id.to_be_bytes());
        // Word 12: CIF0 — Reference Point + Sample Rate + Timestamp Adjustment + CIF1 present.
        let cif0 = Cif::default()
            .with(crate::command::cif0::REFERENCE_POINT, true)
            .with(crate::command::cif0::SAMPLE_RATE, true)
            .with(crate::command::cif0::TIMESTAMP_ADJ, true)
            .with(crate::command::cif0::CIF1_PRESENT, true);
        out.extend_from_slice(&cif0.word().to_be_bytes());
        // Word 13: CIF1 — Buffer Size present (the only CIF1 bit).
        let cif1 = Cif::default().with(crate::command::cif1::BUFFER_SIZE, true);
        out.extend_from_slice(&cif1.word().to_be_bytes());
        // Word 14: Reference Point.
        out.extend_from_slice(&self.reference_point.to_be_bytes());
        // Words 15-16: Sample Rate (u64 scaled 2^20).
        let sr = (self.sample_rate_hz * scale::RADIX_20).round() as i64 as u64;
        out.extend_from_slice(&sr.to_be_bytes());
        // Words 17-18: Timestamp Adjustment (i64 femtoseconds).
        out.extend_from_slice(&self.timestamp_adjust_fs.to_be_bytes());
        // Words 19-20: Buffer Size (u64 bytes).
        out.extend_from_slice(&self.buffer.size_bytes.to_be_bytes());
        // Word 21: Buffer Status (level + OF/NF/NE/UF).
        out.extend_from_slice(&self.buffer.status_word().to_be_bytes());

        debug_assert_eq!(out.len(), command::WORDS as usize * 4);
        Ok(out)
    }

    pub fn unpack(buf: &[u8]) -> Result<TimingFlowControlPacket> {
        let (header, declared_words) = Header::parse(buf)?;
        validate_size(declared_words, buf.len())?;
        if header.packet_type != packet_type::COMMAND {
            return Err(DifiError::BadPacketType {
                actual: header.packet_type,
            });
        }
        let kind = FlowControlKind::from_packet_class(header.packet_class).ok_or(
            DifiError::BadPacketClass {
                actual: header.packet_class,
            },
        )?;
        // Fail-closed structural validation BEFORE interpreting fixed offsets as fields.
        // A Timing Flow Control packet is exactly 21 words; anything else is rejected even
        // if it is self-consistent (an oversized packet must not be silently accepted).
        if declared_words != command::WORDS {
            return Err(DifiError::SizeMismatch {
                declared_words,
                actual_bytes: buf.len(),
            });
        }
        // Class-specific TSF: 0x0005 requires sample-count, 0x0006 real-time ps.
        if header.tsf != kind.tsf() {
            return Err(DifiError::Invalid {
                field: "header.tsf",
                detail: format!(
                    "Packet Class 0x{:04X} requires TSF 0x{:X}, got 0x{:X}",
                    header.packet_class,
                    kind.tsf(),
                    header.tsf
                ),
            });
        }
        let cam = Cam::from_word(read_u32(buf, 28));
        // CAM must declare both IDs present, 32-bit, execute mode (the command profile).
        if !cam.controllee_id_present || !cam.controller_id_present {
            return Err(DifiError::Invalid {
                field: "cam.id_present",
                detail: "Controllee and Controller ID must be present".into(),
            });
        }
        if cam.controllee_id_format || cam.controller_id_format {
            return Err(DifiError::Invalid {
                field: "cam.id_format",
                detail: "ID format must be 32-bit (format bit 0)".into(),
            });
        }
        if cam.action_mode != action_mode::EXECUTE {
            return Err(DifiError::Invalid {
                field: "cam.action_mode",
                detail: "Action Mode must be execute (0b10)".into(),
            });
        }
        let message_id = read_u32(buf, 32);
        let controllee_id = read_u32(buf, 36);
        let controller_id = read_u32(buf, 40);
        // CIF0 (word 12) / CIF1 (word 13) must declare the fields this packet carries, so
        // a packet with cleared CIFs is not silently read as if the fields were present.
        let cif0 = Cif(read_u32(buf, 44));
        let cif1 = Cif(read_u32(buf, 48));
        use crate::command::{cif0 as c0, cif1 as c1};
        let cif0_ok = cif0.get(c0::REFERENCE_POINT)
            && cif0.get(c0::SAMPLE_RATE)
            && cif0.get(c0::TIMESTAMP_ADJ)
            && cif0.get(c0::CIF1_PRESENT);
        if !cif0_ok {
            return Err(DifiError::Invalid {
                field: "cif0",
                detail: format!(
                    "CIF0 0x{:08X} missing required Reference Point / Sample Rate / \
                     Timestamp Adjustment / CIF1-present bits",
                    cif0.word()
                ),
            });
        }
        if !cif1.get(c1::BUFFER_SIZE) {
            return Err(DifiError::Invalid {
                field: "cif1",
                detail: format!("CIF1 0x{:08X} missing Buffer Size present bit", cif1.word()),
            });
        }
        let reference_point = read_u32(buf, 52);
        let sample_rate_hz = (read_u64(buf, 56) as i64) as f64 / scale::RADIX_20;
        let timestamp_adjust_fs = read_u64(buf, 64) as i64;
        let size_bytes = read_u64(buf, 72);
        let status = read_u32(buf, 80);
        let buffer = BufferState::from_status_word(size_bytes, status);
        Ok(TimingFlowControlPacket {
            header,
            kind,
            cam,
            message_id,
            controllee_id,
            controller_id,
            reference_point,
            sample_rate_hz,
            timestamp_adjust_fs,
            buffer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_packet(kind: FlowControlKind) -> TimingFlowControlPacket {
        let header = Header {
            packet_type: packet_type::COMMAND,
            class_id_present: true,
            tsm: tsm::DATA,
            tsi: tsi::UTC,
            tsf: kind.tsf(),
            seq_num: 3,
            stream_id: 0xABCD_1234,
            information_class: information_class::DATA_PLANE_FLOW_CONTROL,
            packet_class: kind.packet_class(),
            integer_seconds: 1_700_000_000,
            fractional_seconds: 12_345_678,
        };
        let cam = Cam {
            controllee_id_present: true,
            controller_id_present: true,
            action_mode: action_mode::EXECUTE,
            ..Cam::default()
        };
        TimingFlowControlPacket {
            header,
            kind,
            cam,
            message_id: 7,
            controllee_id: 0x1111_2222,
            controller_id: 0x3333_4444,
            reference_point: command::REF_POINT,
            sample_rate_hz: 30_720_000.0,
            timestamp_adjust_fs: -5000,
            buffer: BufferState {
                size_bytes: 1 << 22,
                near_full: true,
                level: 0x0ABC, // 12-bit fill level
                ..BufferState::default()
            },
        }
    }

    #[test]
    fn round_trip_sample_count() {
        let p = sample_packet(FlowControlKind::SampleCount);
        let bytes = p.pack().unwrap();
        assert_eq!(bytes.len(), 84);
        let back = TimingFlowControlPacket::unpack(&bytes).unwrap();
        assert_eq!(back.kind, FlowControlKind::SampleCount);
        assert_eq!(back.header.packet_class, packet_class::SAMPLE_COUNT_FLOW_CONTROL);
        assert_eq!(back.header.tsf, tsf::SAMPLE_COUNT);
        assert_eq!(back.message_id, 7);
        assert_eq!(back.controllee_id, 0x1111_2222);
        assert_eq!(back.controller_id, 0x3333_4444);
        assert_eq!(back.reference_point, command::REF_POINT);
        assert!((back.sample_rate_hz - 30_720_000.0).abs() < 1e-6);
        assert_eq!(back.timestamp_adjust_fs, -5000);
        assert_eq!(back.buffer, p.buffer);
        assert_eq!(back.cam, p.cam);
    }

    #[test]
    fn round_trip_real_time() {
        let p = sample_packet(FlowControlKind::RealTime);
        let bytes = p.pack().unwrap();
        let back = TimingFlowControlPacket::unpack(&bytes).unwrap();
        assert_eq!(back.kind, FlowControlKind::RealTime);
        assert_eq!(back.header.packet_class, packet_class::REAL_TIME_FLOW_CONTROL);
        assert_eq!(back.header.tsf, tsf::REAL_TIME_PS);
    }

    #[test]
    fn buffer_status_flags_survive() {
        let mut p = sample_packet(FlowControlKind::RealTime);
        p.buffer = BufferState {
            size_bytes: 9999,
            overflow: true,
            underflow: true,
            near_full: false,
            near_empty: true,
            level: 0x0FFF, // max 12-bit fill level
        };
        let back = TimingFlowControlPacket::unpack(&p.pack().unwrap()).unwrap();
        assert!(back.buffer.overflow);
        assert!(back.buffer.underflow);
        assert!(!back.buffer.near_full);
        assert!(back.buffer.near_empty);
        assert_eq!(back.buffer.level, 0x0FFF);
    }

    #[test]
    fn fail_closed_on_wrong_class() {
        // A COMMAND packet whose class is not a flow-control class is rejected.
        let mut p = sample_packet(FlowControlKind::SampleCount);
        let mut bytes = p.pack().unwrap();
        // Overwrite the packet-class (low 16 bits of word 3, bytes 14-15) with 0x0099.
        bytes[14] = 0x00;
        bytes[15] = 0x99;
        assert!(matches!(
            TimingFlowControlPacket::unpack(&bytes),
            Err(DifiError::BadPacketClass { actual: 0x0099 })
        ));
        // And a size mismatch is rejected.
        p.header.stream_id = 1;
        let short = &p.pack().unwrap()[..80];
        assert!(TimingFlowControlPacket::unpack(short).is_err());
    }

    #[test]
    fn pack_derives_information_class_from_kind() {
        // Even if the caller set a wrong/zero info class, pack() derives the correct one.
        let mut p = sample_packet(FlowControlKind::RealTime);
        p.header.information_class = information_class::DATA_PLANE_FLOW_CONTROL; // wrong (0x0002)
        let back = TimingFlowControlPacket::unpack(&p.pack().unwrap()).unwrap();
        assert_eq!(
            back.header.information_class,
            information_class::DATA_PLANE_FLOW_CONTROL_RT // 0x0003, derived from RealTime
        );
    }

    #[test]
    fn pack_rejects_bad_cam() {
        // Missing an ID-present flag.
        let mut p = sample_packet(FlowControlKind::SampleCount);
        p.cam.controller_id_present = false;
        assert!(p.pack().is_err());
        // Wrong action mode.
        let mut p = sample_packet(FlowControlKind::SampleCount);
        p.cam.action_mode = action_mode::DRY_RUN;
        assert!(p.pack().is_err());
    }

    #[test]
    fn unpack_rejects_cleared_cif0() {
        let p = sample_packet(FlowControlKind::SampleCount);
        let mut bytes = p.pack().unwrap();
        // Zero CIF0 (word 12, bytes 44-47).
        bytes[44..48].copy_from_slice(&0u32.to_be_bytes());
        assert!(TimingFlowControlPacket::unpack(&bytes).is_err());
    }

    #[test]
    fn unpack_rejects_wrong_tsf() {
        let p = sample_packet(FlowControlKind::SampleCount);
        let mut bytes = p.pack().unwrap();
        // Flip TSF in word 0 (bits 21-20) to real-time (0b10) — inconsistent with 0x0005.
        let mut w0 = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        w0 = (w0 & !(0b11 << 20)) | (0b10 << 20);
        bytes[0..4].copy_from_slice(&w0.to_be_bytes());
        assert!(TimingFlowControlPacket::unpack(&bytes).is_err());
    }
}
