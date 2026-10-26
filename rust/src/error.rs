// SPDX-License-Identifier: Apache-2.0
//! Typed, field-level errors. The decoder is fail-closed: every DIFI-profile
//! violation produces a precise error rather than a best-effort mis-parse.

use std::fmt;

/// Error raised when encoding or decoding a DIFI packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DifiError {
    /// Buffer was shorter than required to read a field.
    /// `need` bytes were required at `field`, but only `have` were present.
    Truncated {
        field: &'static str,
        need: usize,
        have: usize,
    },
    /// The OUI did not match the VITA OUI (0x6A621E).
    BadOui { expected: u32, actual: u32 },
    /// The Packet Size field disagreed with the actual buffer length.
    SizeMismatch {
        declared_words: u16,
        actual_bytes: usize,
    },
    /// The VRT packet-type nibble was not a DIFI 1.1 type.
    BadPacketType { actual: u8 },
    /// The Packet Class Code was not a recognised DIFI 1.1 class.
    BadPacketClass { actual: u16 },
    /// A bit depth outside the supported 4..=16 range was requested.
    BadBitDepth { actual: u8 },
    /// A configuration would not yield a whole number of 4-byte words.
    NotWordAligned { field: &'static str, value: usize },
    /// The payload would exceed the DIFI maximum octet count.
    PayloadTooLarge { octets: usize, max: usize },
    /// A sample value did not fit in the chosen signed bit depth.
    SampleOutOfRange { value: i32, bit_depth: u8 },
    /// Generic field-level validation failure.
    Invalid { field: &'static str, detail: String },
}

impl fmt::Display for DifiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DifiError::Truncated { field, need, have } => write!(
                f,
                "truncated at field '{field}': need {need} bytes, have {have}"
            ),
            DifiError::BadOui { expected, actual } => write!(
                f,
                "bad OUI: expected 0x{expected:06X}, got 0x{actual:06X} (not a DIFI packet)"
            ),
            DifiError::SizeMismatch {
                declared_words,
                actual_bytes,
            } => write!(
                f,
                "packet-size mismatch: declared {declared_words} words ({} bytes), buffer is {actual_bytes} bytes",
                *declared_words as usize * 4
            ),
            DifiError::BadPacketType { actual } => {
                write!(f, "unsupported VRT packet type 0x{actual:X}")
            }
            DifiError::BadPacketClass { actual } => {
                write!(f, "unrecognised packet class code 0x{actual:04X}")
            }
            DifiError::BadBitDepth { actual } => {
                write!(f, "unsupported bit depth {actual} (must be 4..=16)")
            }
            DifiError::NotWordAligned { field, value } => {
                write!(f, "'{field}' = {value} is not 4-byte-word aligned")
            }
            DifiError::PayloadTooLarge { octets, max } => {
                write!(f, "payload {octets} octets exceeds DIFI maximum {max}")
            }
            DifiError::SampleOutOfRange { value, bit_depth } => {
                write!(f, "sample {value} does not fit in {bit_depth}-bit signed")
            }
            DifiError::Invalid { field, detail } => {
                write!(f, "invalid '{field}': {detail}")
            }
        }
    }
}

impl std::error::Error for DifiError {}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, DifiError>;
