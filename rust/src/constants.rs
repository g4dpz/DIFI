// SPDX-License-Identifier: Apache-2.0
//! DIFI wire-level constants (IEEE-ISTO Std 4900-2021).
//!
//! Covers the DIFI 1.1 data plane plus the DIFI 1.2 additions (sample-count classes and
//! Timing Flow Control). These values MUST be identical across every independent
//! implementation of this toolkit (Rust, C++). They are the shared definition of the
//! DIFI profile; see `docs/reference/DIFI-VITA49-PRIMER.md` section 3.

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
///
/// DIFI 1.2 adds the Command packet type:
/// - Command packet with Stream ID    = 0x6  (Timing Flow Control)
///
/// (DIFI 1.3.0 adds Extension Command `0x7`; that belongs to the separate
/// link-establishment feature, not this one.)
pub mod packet_type {
    /// Signal Data packet with Stream Identifier.
    pub const SIGNAL_DATA: u8 = 0x1;
    /// Context packet (used by both Signal Context and Version Context).
    pub const CONTEXT: u8 = 0x4;
    /// Command packet with Stream Identifier (DIFI 1.2 Timing Flow Control).
    pub const COMMAND: u8 = 0x6;
}

/// VRT packet-type nibble for the Version Context packet.
///
/// Per the DIFI certification definitions, the Version Context packet uses VRT packet
/// type 0x5 (distinct from Signal Context's 0x4).
pub mod version_packet_type {
    pub const VERSION_CONTEXT: u8 = 0x5;
}

/// DIFI Information Class Codes.
///
/// An Information Class names the *set of Packet Classes* that make up an Information
/// Stream; it is NOT a packet layout. These values live in the high 16 bits of header
/// word 3 and share numeric values with [`packet_class`] codes (which live in the low
/// 16 bits) — the two are different fields and must not be conflated.
pub mod information_class {
    /// Standard Flow (Signal Data + Signal Context) — DIFI 1.1 "Basic Data Plane".
    pub const STANDARD_FLOW: u16 = 0x0000;
    /// Version Flow (Version Context packet) — DIFI 1.1.
    pub const VERSION_FLOW: u16 = 0x0001;
    /// Data Plane plus Flow Control (adds Timing Flow Control stream) — DIFI 1.2.0.
    pub const DATA_PLANE_FLOW_CONTROL: u16 = 0x0002;
    /// Data Plane plus Flow Control, Real-Time TSF variant — DIFI 1.2.0.
    pub const DATA_PLANE_FLOW_CONTROL_RT: u16 = 0x0003;
    /// Second "Basic Data Plane" grouping (same streams as STANDARD_FLOW) — DIFI 1.2.1.
    pub const BASIC_DATA_PLANE_2: u16 = 0x0004;
}

/// Legacy alias (Standard Flow). Prefer [`information_class`].
pub const INFORMATION_CLASS_CODE: u16 = information_class::STANDARD_FLOW;

/// DIFI Packet Class Codes (identify a packet's wire layout).
///
/// These live in the low 16 bits of header word 3. Per the authoritative Table 4-1.
/// Note the numeric overlap with [`information_class`] codes is expected — they are
/// different header fields.
pub mod packet_class {
    /// Standard Flow Signal Data (type 0x1) — DIFI 1.1.
    pub const SIGNAL_DATA: u16 = 0x0000;
    /// Standard Flow Signal Context (type 0x4) — DIFI 1.1.
    pub const SIGNAL_CONTEXT: u16 = 0x0001;
    /// Sample Count Signal Data (type 0x1) — DIFI 1.2. Same layout as [`SIGNAL_DATA`],
    /// timestamped with a sample count (TSF = sample count).
    pub const SAMPLE_COUNT_SIGNAL_DATA: u16 = 0x0002;
    /// Sample Count Signal Context (type 0x4) — DIFI 1.2. Same layout as
    /// [`SIGNAL_CONTEXT`], sample-count timestamped.
    pub const SAMPLE_COUNT_SIGNAL_CONTEXT: u16 = 0x0003;
    /// Version Flow Context (type 0x4) — DIFI 1.1.
    pub const VERSION_CONTEXT: u16 = 0x0004;
    /// Sample Count Timing Flow Control (Command, type 0x6) — DIFI 1.2.
    pub const SAMPLE_COUNT_FLOW_CONTROL: u16 = 0x0005;
    /// Real-Time TSF Timing Flow Control (Command, type 0x6) — DIFI 1.2.
    pub const REAL_TIME_FLOW_CONTROL: u16 = 0x0006;
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

/// Fixed field values for the DIFI 1.2 Timing Flow Control (Command) packet.
pub mod command {
    /// Fixed packet size in 32-bit words (84 bytes) for the base Timing Flow Control
    /// packet (header + CAM/CIF/IDs + Reference Point + Sample Rate + Timestamp
    /// Adjustment + Buffer Size + Buffer Status).
    pub const WORDS: u16 = 21;
    /// Reference Point default (same value as the context Reference Point).
    pub const REF_POINT: u32 = 0x0000_0064;
}

/// Buffer Status flag bit positions in the Timing Flow Control packet's status word
/// (word 21, low nibble). Each is a single bit. Names carry a `_BIT` suffix to match the
/// C++ implementation, where `<cmath>` predefines `OVERFLOW`/`UNDERFLOW` as macros.
pub mod buffer_status {
    /// Overflow (OF) — bit 3.
    pub const OVERFLOW_BIT: u8 = 3;
    /// Near-Full (NF) — bit 2.
    pub const NEAR_FULL_BIT: u8 = 2;
    /// Near-Empty (NE) — bit 1.
    pub const NEAR_EMPTY_BIT: u8 = 1;
    /// Underflow (UF) — bit 0.
    pub const UNDERFLOW_BIT: u8 = 0;
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
