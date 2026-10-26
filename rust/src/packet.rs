//! DIFI 1.1 packet model: Signal Data, Signal Context, Version Context.
//!
//! All multi-byte fields are big-endian and 32-bit-word aligned (primer section 3).
//! The common header is 7 words (28 bytes):
//!
//! ```text
//! word 0: [ptype:4][C:1][indicators:3][TSI:2][TSF:2][seq:4][packet_size:16]
//! word 1: Stream ID (32)
//! word 2: [reserved:8][OUI:24]
//! word 3: [Information Class Code:16][Packet Class Code:16]
//! word 4: Integer-seconds timestamp (32)
//! word 5: Fractional-seconds timestamp high (32)
//! word 6: Fractional-seconds timestamp low  (32)
//! ```

use crate::constants::*;
use crate::error::{DifiError, Result};
use crate::iq::{pack_samples, payload_word_count, unpack_samples, IqSample};

/// Common DIFI header fields shared by all three packet types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub packet_type: u8,
    pub class_id_present: bool,
    /// TSM (time-stamp mode) bit: 0 for Signal Data, 1 for Context/Version.
    pub tsm: u8,
    pub tsi: u8,
    pub tsf: u8,
    pub seq_num: u8, // mod-16
    pub stream_id: u32,
    pub information_class: u16,
    pub packet_class: u16,
    pub integer_seconds: u32,
    pub fractional_seconds: u64,
}

impl Header {
    fn write_into(&self, packet_size_words: u16, out: &mut Vec<u8>) {
        let word0: u32 = ((self.packet_type as u32 & 0xF) << 28)
            | ((self.class_id_present as u32) << 27)
            // bits 25-26 reserved = 0
            | ((self.tsm as u32 & 0x1) << 24)
            | ((self.tsi as u32 & 0x3) << 22)
            | ((self.tsf as u32 & 0x3) << 20)
            | ((self.seq_num as u32 & 0xF) << 16)
            | (packet_size_words as u32 & 0xFFFF);
        out.extend_from_slice(&word0.to_be_bytes());
        out.extend_from_slice(&self.stream_id.to_be_bytes());
        out.extend_from_slice(&(VITA_OUI & 0x00FF_FFFF).to_be_bytes());
        let word3 = ((self.information_class as u32) << 16) | self.packet_class as u32;
        out.extend_from_slice(&word3.to_be_bytes());
        out.extend_from_slice(&self.integer_seconds.to_be_bytes());
        out.extend_from_slice(&self.fractional_seconds.to_be_bytes());
    }

    /// Parse the fixed 28-byte header, fail-closed. Returns (header, declared_words).
    fn parse(buf: &[u8]) -> Result<(Header, u16)> {
        if buf.len() < HEADER_BYTES {
            return Err(DifiError::Truncated {
                field: "header",
                need: HEADER_BYTES,
                have: buf.len(),
            });
        }
        let word0 = read_u32(buf, 0);
        let packet_type = ((word0 >> 28) & 0xF) as u8;
        let class_id_present = (word0 >> 27) & 1 == 1;
        let tsm = ((word0 >> 24) & 0x1) as u8;
        let tsi = ((word0 >> 22) & 0x3) as u8;
        let tsf = ((word0 >> 20) & 0x3) as u8;
        let seq_num = ((word0 >> 16) & 0xF) as u8;
        let packet_size = (word0 & 0xFFFF) as u16;

        let stream_id = read_u32(buf, 4);
        let oui = read_u32(buf, 8) & 0x00FF_FFFF;
        if oui != (VITA_OUI & 0x00FF_FFFF) {
            return Err(DifiError::BadOui {
                expected: VITA_OUI & 0x00FF_FFFF,
                actual: oui,
            });
        }
        let word3 = read_u32(buf, 12);
        let information_class = (word3 >> 16) as u16;
        let packet_class = (word3 & 0xFFFF) as u16;
        let integer_seconds = read_u32(buf, 16);
        let fractional_seconds = read_u64(buf, 20);

        Ok((
            Header {
                packet_type,
                class_id_present,
                tsm,
                tsi,
                tsf,
                seq_num,
                stream_id,
                information_class,
                packet_class,
                integer_seconds,
                fractional_seconds,
            },
            packet_size,
        ))
    }
}

/// A DIFI 1.1 Signal Data packet (header + I/Q payload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalDataPacket {
    pub header: Header,
    pub bit_depth: u8,
    pub samples: Vec<IqSample>,
}

impl SignalDataPacket {
    pub fn pack(&self) -> Result<Vec<u8>> {
        let payload_words = payload_word_count(self.samples.len(), self.bit_depth)?;
        let total_words = HEADER_WORDS as usize + payload_words;
        if total_words > u16::MAX as usize {
            return Err(DifiError::Invalid {
                field: "packet_size",
                detail: format!("{total_words} words exceeds 16-bit field"),
            });
        }
        let payload_octets = total_words * 4;
        if payload_octets > MAX_PAYLOAD_OCTETS {
            return Err(DifiError::PayloadTooLarge {
                octets: payload_octets,
                max: MAX_PAYLOAD_OCTETS,
            });
        }
        let payload = pack_samples(&self.samples, self.bit_depth)?;
        let mut out = Vec::with_capacity(total_words * 4);
        self.header.write_into(total_words as u16, &mut out);
        out.extend_from_slice(&payload);
        Ok(out)
    }

    /// Decode a Signal Data packet. `bit_depth` is supplied out-of-band because it
    /// lives in the preceding Signal Context packet (it is not in the data header).
    pub fn unpack(buf: &[u8], bit_depth: u8) -> Result<SignalDataPacket> {
        let (header, declared_words) = Header::parse(buf)?;
        validate_size(declared_words, buf.len())?;
        if header.packet_type != packet_type::SIGNAL_DATA {
            return Err(DifiError::BadPacketType {
                actual: header.packet_type,
            });
        }
        let payload = &buf[HEADER_BYTES..declared_words as usize * 4];
        let payload_words = payload.len() / 4;
        // samples = payload_words * 32 / (2 * bit_depth)
        let total_bits = payload_words * 32;
        if total_bits % (2 * bit_depth as usize) != 0 {
            return Err(DifiError::NotWordAligned {
                field: "payload bits / (2*bit_depth)",
                value: total_bits,
            });
        }
        let sample_count = total_bits / (2 * bit_depth as usize);
        let samples = unpack_samples(payload, sample_count, bit_depth)?;
        Ok(SignalDataPacket {
            header,
            bit_depth,
            samples,
        })
    }
}

/// DIFI sample data format descriptor carried in a Signal Context packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataFormat {
    /// Signed-integer bit depth (4..=16).
    pub bit_depth: u8,
}

/// A DIFI 1.1 Signal Context packet (stream metadata), conformant to the DIFI
/// certification `difi_context_v1_1` definition: a fixed 27-word packet.
///
/// Many VITA-49 context fields are fixed by the DIFI profile and not independently
/// meaningful here; they are held as constants on `pack`. The semantically useful
/// fields are exposed on the struct. Frequencies/bandwidth/sample rate are stored as
/// fixed-point `value * 2^20`; gains/reference levels as `value * 2^7`.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalContextPacket {
    pub header: Header,
    /// Whether the context-changed bit is set in CIF0.
    pub context_changed: bool,
    pub rf_reference_hz: f64,
    pub if_reference_hz: f64,
    pub bandwidth_hz: f64,
    pub sample_rate_hz: f64,
    /// Stage-1 gain / attenuation in dB (maps to `stage1GainAtten`).
    pub gain_db: f64,
    pub data_format: DataFormat,
}

#[inline]
fn to_fixed_i64(value: f64, radix: f64) -> i64 {
    (value * radix).round() as i64
}
#[inline]
fn from_fixed_i64(raw: i64, radix: f64) -> f64 {
    raw as f64 / radix
}
#[inline]
fn to_fixed_i16(value: f64, radix: f64) -> i16 {
    (value * radix).round() as i16
}
#[inline]
fn from_fixed_i16(raw: i16, radix: f64) -> f64 {
    raw as f64 / radix
}

impl SignalContextPacket {
    pub fn pack(&self) -> Result<Vec<u8>> {
        crate::iq::check_bit_depth(self.data_format.bit_depth)?;
        let total_words = context::WORDS;
        let mut out = Vec::with_capacity(total_words as usize * 4);
        self.header.write_into(total_words, &mut out);

        // cif0, refPoint
        let cif0 = if self.context_changed {
            context::CIF0_CHANGED
        } else {
            context::CIF0_NO_CHANGE
        };
        out.extend_from_slice(&cif0.to_be_bytes());
        out.extend_from_slice(&context::REF_POINT.to_be_bytes());
        // bandwidth(u64 scaled), ifFreq(i64), rfFreq(i64), ifBandOffset(i64)
        out.extend_from_slice(&(to_fixed_i64(self.bandwidth_hz, scale::RADIX_20) as u64).to_be_bytes());
        out.extend_from_slice(&to_fixed_i64(self.if_reference_hz, scale::RADIX_20).to_be_bytes());
        out.extend_from_slice(&to_fixed_i64(self.rf_reference_hz, scale::RADIX_20).to_be_bytes());
        out.extend_from_slice(&0i64.to_be_bytes()); // ifBandOffset
        // refLevel1, refLevel2, stage1GainAtten, stage2GainAtten (i16 scaled)
        out.extend_from_slice(&0i16.to_be_bytes());
        out.extend_from_slice(&0i16.to_be_bytes());
        out.extend_from_slice(&to_fixed_i16(self.gain_db, scale::RADIX_7).to_be_bytes());
        out.extend_from_slice(&0i16.to_be_bytes());
        // sampleRate(u64 scaled)
        out.extend_from_slice(&(to_fixed_i64(self.sample_rate_hz, scale::RADIX_20) as u64).to_be_bytes());
        // timeStampAdj(i64), timeStampCal(u32)
        out.extend_from_slice(&0i64.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        // stateEventInd (1 word) — all zero
        out.extend_from_slice(&0u32.to_be_bytes());
        // dataPacketFormat (2 words) — encode bit depth, link-efficient, complex cartesian
        let (df0, df1) = encode_data_packet_format(self.data_format.bit_depth);
        out.extend_from_slice(&df0.to_be_bytes());
        out.extend_from_slice(&df1.to_be_bytes());

        debug_assert_eq!(out.len(), context::WORDS as usize * 4);
        Ok(out)
    }

    pub fn unpack(buf: &[u8]) -> Result<SignalContextPacket> {
        let (header, declared_words) = Header::parse(buf)?;
        validate_size(declared_words, buf.len())?;
        if header.packet_type != packet_type::CONTEXT {
            return Err(DifiError::BadPacketType {
                actual: header.packet_type,
            });
        }
        if header.packet_class != packet_class::SIGNAL_CONTEXT {
            return Err(DifiError::BadPacketClass {
                actual: header.packet_class,
            });
        }
        let need = context::WORDS as usize * 4;
        if buf.len() < need {
            return Err(DifiError::Truncated {
                field: "signal-context payload",
                need,
                have: buf.len(),
            });
        }
        let cif0 = read_u32(buf, 28);
        let context_changed = cif0 == context::CIF0_CHANGED;
        // refPoint at 32
        let bandwidth_hz = from_fixed_i64(read_u64(buf, 36) as i64, scale::RADIX_20);
        let if_reference_hz = from_fixed_i64(read_i64(buf, 44), scale::RADIX_20);
        let rf_reference_hz = from_fixed_i64(read_i64(buf, 52), scale::RADIX_20);
        // ifBandOffset at 60
        // refLevel1 at 68, refLevel2 at 70
        let gain_db = from_fixed_i16(read_i16(buf, 72), scale::RADIX_7);
        // stage2 at 74
        let sample_rate_hz = from_fixed_i64(read_u64(buf, 76) as i64, scale::RADIX_20);
        // timeStampAdj at 84, timeStampCal at 92, stateEventInd at 96
        let df0 = read_u32(buf, 100);
        let df1 = read_u32(buf, 104);
        let bit_depth = decode_data_packet_format(df0, df1)?;
        Ok(SignalContextPacket {
            header,
            context_changed,
            rf_reference_hz,
            if_reference_hz,
            bandwidth_hz,
            sample_rate_hz,
            gain_db,
            data_format: DataFormat { bit_depth },
        })
    }
}

/// Encode the 2-word `dataPacketFormat` field. Bit depth is carried in both
/// `item_packing_field_size` and `data_item_size` as `(bits - 1)`.
fn encode_data_packet_format(bit_depth: u8) -> (u32, u32) {
    let packing_method = 1u32; // link_efficient
    let real_complex = 1u32; // complex_cartesian
    let data_item_format = 0u32; // signed_fixed_point
    let sample_repeat = 0u32;
    let event_tag = 0u32;
    let channel_tag = 0u32;
    let data_item_fraction = 0u32;
    let ipfs = (bit_depth as u32 - 1) & 0x3F; // item_packing_field_size (6 bits)
    let dis = (bit_depth as u32 - 1) & 0x3F; // data_item_size (6 bits)
    let word0 = (packing_method << 31)
        | (real_complex << 29)
        | (data_item_format << 24)
        | (sample_repeat << 23)
        | (event_tag << 20)
        | (channel_tag << 16)
        | (data_item_fraction << 12)
        | (ipfs << 6)
        | dis;
    let repeat_count = 0u32;
    let vector_size = 0u32;
    let word1 = (repeat_count << 16) | vector_size;
    (word0, word1)
}

fn decode_data_packet_format(word0: u32, _word1: u32) -> Result<u8> {
    let dis = (word0 & 0x3F) as u8;
    let bit_depth = dis + 1;
    crate::iq::check_bit_depth(bit_depth)?;
    Ok(bit_depth)
}

/// A DIFI 1.1 Version Context packet, conformant to the certification
/// `difi_version_v1_1` definition (fixed 11-word packet, VRT packet type 0x5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionContextPacket {
    pub header: Header,
    pub context_changed: bool,
    /// Year the firmware/standard version was issued (full year, e.g. 2021).
    pub year: u16,
    /// Day of year (1-366).
    pub day: u16,
}

impl VersionContextPacket {
    pub fn pack(&self) -> Result<Vec<u8>> {
        let total_words = version::WORDS;
        let mut out = Vec::with_capacity(total_words as usize * 4);
        self.header.write_into(total_words, &mut out);
        let cif0 = if self.context_changed {
            version::CIF0_CHANGED
        } else {
            version::CIF0_NO_CHANGE
        };
        out.extend_from_slice(&cif0.to_be_bytes());
        out.extend_from_slice(&version::CIF1.to_be_bytes());
        out.extend_from_slice(&version::V49_SPEC_VERSION.to_be_bytes());
        // versionInfo: year(7, since 2000) day(9) revision(6=1) type(4=0) icdVersion(6=0)
        let year_field = (self.year as i32 - 2000).clamp(0, 127) as u32;
        let version_info = (year_field << 25)
            | ((self.day as u32 & 0x1FF) << 16)
            | (1u32 << 10) // revision = 1
            | (0u32 << 6)  // type = 0
            | 0u32; // icdVersion = 0
        out.extend_from_slice(&version_info.to_be_bytes());
        debug_assert_eq!(out.len(), version::WORDS as usize * 4);
        Ok(out)
    }

    pub fn unpack(buf: &[u8]) -> Result<VersionContextPacket> {
        let (header, declared_words) = Header::parse(buf)?;
        validate_size(declared_words, buf.len())?;
        if header.packet_type != version_packet_type::VERSION_CONTEXT {
            return Err(DifiError::BadPacketType {
                actual: header.packet_type,
            });
        }
        if header.packet_class != packet_class::VERSION_CONTEXT {
            return Err(DifiError::BadPacketClass {
                actual: header.packet_class,
            });
        }
        let need = version::WORDS as usize * 4;
        if buf.len() < need {
            return Err(DifiError::Truncated {
                field: "version-context payload",
                need,
                have: buf.len(),
            });
        }
        let cif0 = read_u32(buf, 28);
        let context_changed = cif0 == version::CIF0_CHANGED;
        let version_info = read_u32(buf, 40);
        let year = ((version_info >> 25) & 0x7F) as u16 + 2000;
        let day = ((version_info >> 16) & 0x1FF) as u16;
        Ok(VersionContextPacket {
            header,
            context_changed,
            year,
            day,
        })
    }
}

/// Reject a Packet-Size/length mismatch (Req 1.5): the declared word count MUST equal
/// the actual buffer length in words, and the buffer must be word-aligned.
fn validate_size(declared_words: u16, actual_bytes: usize) -> Result<()> {
    if actual_bytes % 4 != 0 {
        return Err(DifiError::NotWordAligned {
            field: "packet length",
            value: actual_bytes,
        });
    }
    if declared_words as usize * 4 != actual_bytes {
        return Err(DifiError::SizeMismatch {
            declared_words,
            actual_bytes,
        });
    }
    Ok(())
}

#[inline]
fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

#[inline]
fn read_u64(buf: &[u8], off: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&buf[off..off + 8]);
    u64::from_be_bytes(b)
}

#[inline]
fn read_i64(buf: &[u8], off: usize) -> i64 {
    read_u64(buf, off) as i64
}

#[inline]
fn read_i16(buf: &[u8], off: usize) -> i16 {
    i16::from_be_bytes([buf[off], buf[off + 1]])
}

