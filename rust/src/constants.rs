//! DIFI 1.1 wire-level constants (IEEE-ISTO Std 4900-2021).
//!
//! These values MUST be identical across every independent implementation of this
//! toolkit (Rust, C++). They are the shared definition of the DIFI 1.1 profile; see
//! `docs/reference/DIFI-VITA49-PRIMER.md` section 3.

/// VITA Organizationally Unique Identifier used by every DIFI packet (24-bit).
pub const VITA_OUI: u32 = 0x006A_621E;

/// Fixed DIFI header length, in 32-bit words (7 words = 28 bytes).
pub const HEADER_WORDS: u16 = 7;
/// Fixed DIFI header length, in bytes.
pub const HEADER_BYTES: usize = HEADER_WORDS as usize * 4;

/// Maximum UDP payload size for a DIFI packet, in octets (~9000).
pub const MAX_PAYLOAD_OCTETS: usize = 9000;

/// VRT packet-type nibble (top 4 bits of word 0).
///
/// DIFI 1.1 uses two VRT packet types:
/// - Signal Data with Stream ID       = 0x1
/// - Context (extension data) packet  = 0x4
pub mod packet_type {
    /// Signal Data packet with Stream Identifier.
    pub const SIGNAL_DATA: u8 = 0x1;
    /// Context packet (used by both Signal Context and Version Context).
    pub const CONTEXT: u8 = 0x4;
}

/// VRT packet-type nibble for the Version Context packet.
///
/// Per the DIFI certification definitions, the Version Context packet uses VRT packet
/// type 0x5 (distinct from Signal Context's 0x4).
pub mod version_packet_type {
    pub const VERSION_CONTEXT: u8 = 0x5;
}

/// DIFI 1.1 Information Class Codes.
pub mod information_class {
    /// Standard Flow (data + signal context) information class.
    pub const STANDARD_FLOW: u16 = 0x0000;
    /// Version Flow information class.
    pub const VERSION_FLOW: u16 = 0x0001;
}

/// Legacy alias (Standard Flow). Prefer [`information_class`].
pub const INFORMATION_CLASS_CODE: u16 = information_class::STANDARD_FLOW;

/// DIFI 1.1 Packet Class Codes (distinguish the three packet kinds).
pub mod packet_class {
    /// Standard Flow Signal Data packet class.
    pub const SIGNAL_DATA: u16 = 0x0000;
    /// Standard Flow Signal Context packet class.
    pub const SIGNAL_CONTEXT: u16 = 0x0001;
    /// Version Flow Context packet class.
    pub const VERSION_CONTEXT: u16 = 0x0004;
}

/// Fixed field values required by the DIFI 1.1 Signal Context packet.
pub mod context {
    /// Fixed packet size in 32-bit words (108 bytes).
    pub const WORDS: u16 = 27;
    /// CIF0 with the context-changed bit set.
    pub const CIF0_CHANGED: u32 = 0xFBB9_8000;
    /// CIF0 without the context-changed bit.
    pub const CIF0_NO_CHANGE: u32 = 0x7BB9_8000;
    /// Reference point ID.
    pub const REF_POINT: u32 = 0x0000_0064;
}

/// Fixed field values required by the DIFI 1.1 Version Context packet.
pub mod version {
    /// Fixed packet size in 32-bit words (44 bytes).
    pub const WORDS: u16 = 11;
    /// CIF0 with the context-changed bit set.
    pub const CIF0_CHANGED: u32 = 0x8000_0002;
    /// CIF0 without the context-changed bit.
    pub const CIF0_NO_CHANGE: u32 = 0x0000_0002;
    /// Required CIF1 value.
    pub const CIF1: u32 = 0x0000_000C;
    /// Required V49 spec-version value.
    pub const V49_SPEC_VERSION: u32 = 0x0000_0004;
}

/// Scaling for the context fixed-point fields (construct ExprAdapter equivalents).
pub mod scale {
    /// Frequencies, bandwidth and sample rate are stored as integer * 2^20.
    pub const RADIX_20: f64 = 1_048_576.0; // 2^20
    /// Reference levels and gains are stored as integer * 2^7.
    pub const RADIX_7: f64 = 128.0; // 2^7
}

/// TSM (time-stamp mode) bit (bit 24 of word 0).
pub mod tsm {
    pub const DATA: u8 = 0; // Signal Data requires TSM = 0
    pub const CONTEXT: u8 = 1; // Context / Version require TSM = 1
}

/// Timestamp-Integer (TSI) field values (bits 23-22 of word 0).
pub mod tsi {
    /// No integer timestamp.
    pub const NONE: u8 = 0b00;
    /// UTC integer-seconds timestamp (DIFI default).
    pub const UTC: u8 = 0b01;
    /// GPS integer-seconds timestamp.
    pub const GPS: u8 = 0b10;
    /// Other integer-seconds timestamp.
    pub const OTHER: u8 = 0b11;
}

/// Timestamp-Fractional (TSF) field values (bits 21-20 of word 0).
pub mod tsf {
    /// No fractional timestamp.
    pub const NONE: u8 = 0b00;
    /// Sample-count fractional timestamp.
    pub const SAMPLE_COUNT: u8 = 0b01;
    /// Real-time (picoseconds) fractional timestamp (DIFI default).
    pub const REAL_TIME_PS: u8 = 0b10;
    /// Free-running count fractional timestamp.
    pub const FREE_RUNNING: u8 = 0b11;
}

/// Supported signed-integer sample bit-depth range (inclusive), per DIFI.
pub const MIN_BIT_DEPTH: u8 = 4;
pub const MAX_BIT_DEPTH: u8 = 16;
