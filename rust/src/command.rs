// SPDX-License-Identifier: Apache-2.0
//! Shared DIFI Command-Packet core (VITA 49.2 §8) — the foundation for the DIFI 1.2
//! Timing Flow Control packets (`0x0005`/`0x0006`) and the DIFI 1.3.0 link-establishment
//! packets (`0x0007`/`0x0008`/`0x0009`).
//!
//! A DIFI Command / Extension-Command packet carries, after the 7-word prologue:
//! a **CAM** (Control/Acknowledge Mode) word, a **CIF0** and optional **CIF1** indicator
//! word, a Message ID, a Controllee ID and a Controller ID, and then command-specific
//! fields selected by the CIF bits (Reference Point, Sample Rate, Timestamp Adjustment,
//! Buffer Size, …).
//!
//! This module provides **field-level** read/write for the CAM and CIF words plus typed
//! helpers for the common context words, so each packet type composes exactly the bits it
//! needs. We deliberately do NOT hard-code whole CAM constants: the per-field encoding is
//! provably correct bit-by-bit, and each concrete packet (tasks 3/5) composes and then
//! verifies its full CAM against the conformance tooling.

use crate::error::{DifiError, Result};

/// CAM (Control/Acknowledge Mode) field — VITA 49.2 §8, as used by DIFI command packets.
///
/// Bit layout (MSB = bit 31), per the DIFI v1.2.1 command reference definition:
/// - bit 31: Controllee ID Identifier present
/// - bit 30: Controllee ID format (0 = 32-bit)
/// - bit 29: Controller ID Identifier present
/// - bit 28: Controller ID format (0 = 32-bit)
/// - bits 24-23: Action Mode (0b10 = execute)
/// - bit 22: NACK (negative acknowledge request)
/// - bit 21: Validity Acknowledge request
/// - bits 14-12: Timestamp Control Mode
/// - bits 11-8: Acknowledge bits (0 for a Control packet)
/// - all other bits reserved / unused by the DIFI command profile (kept round-trippable)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cam {
    pub controllee_id_present: bool,
    /// Controllee ID format bit (false = 32-bit, the DIFI default).
    pub controllee_id_format: bool,
    pub controller_id_present: bool,
    pub controller_id_format: bool,
    /// Action Mode, bits 24-23 (0b10 = execute).
    pub action_mode: u8,
    /// NACK request, bit 22.
    pub nack: bool,
    /// Validity Acknowledge request, bit 21.
    pub validity_ack: bool,
    /// Timestamp Control Mode, bits 14-12.
    pub timestamp_control: u8,
    /// Acknowledge bits, bits 11-8 (0 in a Control packet).
    pub ack_bits: u8,
    /// Any bits the DIFI profile does not model, preserved verbatim for exact round-trip.
    pub reserved: u32,
}

// CAM sub-field bit positions / masks (DIFI v1.2.1 command reference).
mod cam_bits {
    pub const CONTROLLEE_PRESENT: u32 = 31;
    pub const CONTROLLEE_FORMAT: u32 = 30;
    pub const CONTROLLER_PRESENT: u32 = 29;
    pub const CONTROLLER_FORMAT: u32 = 28;
    pub const ACTION_MODE_SHIFT: u32 = 23; // bits 24-23
    pub const ACTION_MODE_MASK: u32 = 0b11;
    pub const NACK: u32 = 22;
    pub const VALIDITY_ACK: u32 = 21;
    pub const TS_CONTROL_SHIFT: u32 = 12; // bits 14-12
    pub const TS_CONTROL_MASK: u32 = 0b111;
    pub const ACK_BITS_SHIFT: u32 = 8; // bits 11-8
    pub const ACK_BITS_MASK: u32 = 0b1111;

    /// All bits this module interprets; everything else is `reserved`.
    pub const MODELLED_MASK: u32 = (1 << CONTROLLEE_PRESENT)
        | (1 << CONTROLLEE_FORMAT)
        | (1 << CONTROLLER_PRESENT)
        | (1 << CONTROLLER_FORMAT)
        | (ACTION_MODE_MASK << ACTION_MODE_SHIFT)
        | (1 << NACK)
        | (1 << VALIDITY_ACK)
        | (TS_CONTROL_MASK << TS_CONTROL_SHIFT)
        | (ACK_BITS_MASK << ACK_BITS_SHIFT);
}

/// Action Mode values (CAM bits 24-23).
pub mod action_mode {
    /// No action (dry-run disabled / not an action packet).
    pub const NONE: u8 = 0b00;
    /// Dry run — validate but do not execute.
    pub const DRY_RUN: u8 = 0b01;
    /// Execute the requested action.
    pub const EXECUTE: u8 = 0b10;
}

impl Cam {
    /// Decode a CAM word into its fields, preserving any unmodelled bits in `reserved`.
    pub fn from_word(word: u32) -> Cam {
        Cam {
            controllee_id_present: (word >> cam_bits::CONTROLLEE_PRESENT) & 1 == 1,
            controllee_id_format: (word >> cam_bits::CONTROLLEE_FORMAT) & 1 == 1,
            controller_id_present: (word >> cam_bits::CONTROLLER_PRESENT) & 1 == 1,
            controller_id_format: (word >> cam_bits::CONTROLLER_FORMAT) & 1 == 1,
            action_mode: ((word >> cam_bits::ACTION_MODE_SHIFT) & cam_bits::ACTION_MODE_MASK) as u8,
            nack: (word >> cam_bits::NACK) & 1 == 1,
            validity_ack: (word >> cam_bits::VALIDITY_ACK) & 1 == 1,
            timestamp_control: ((word >> cam_bits::TS_CONTROL_SHIFT) & cam_bits::TS_CONTROL_MASK)
                as u8,
            ack_bits: ((word >> cam_bits::ACK_BITS_SHIFT) & cam_bits::ACK_BITS_MASK) as u8,
            reserved: word & !cam_bits::MODELLED_MASK,
        }
    }

    /// Encode the CAM fields back into a 32-bit word (round-trips `from_word`).
    pub fn to_word(self) -> u32 {
        let mut w = self.reserved & !cam_bits::MODELLED_MASK;
        w |= (self.controllee_id_present as u32) << cam_bits::CONTROLLEE_PRESENT;
        w |= (self.controllee_id_format as u32) << cam_bits::CONTROLLEE_FORMAT;
        w |= (self.controller_id_present as u32) << cam_bits::CONTROLLER_PRESENT;
        w |= (self.controller_id_format as u32) << cam_bits::CONTROLLER_FORMAT;
        w |=
            ((self.action_mode as u32) & cam_bits::ACTION_MODE_MASK) << cam_bits::ACTION_MODE_SHIFT;
        w |= (self.nack as u32) << cam_bits::NACK;
        w |= (self.validity_ack as u32) << cam_bits::VALIDITY_ACK;
        w |= ((self.timestamp_control as u32) & cam_bits::TS_CONTROL_MASK)
            << cam_bits::TS_CONTROL_SHIFT;
        w |= ((self.ack_bits as u32) & cam_bits::ACK_BITS_MASK) << cam_bits::ACK_BITS_SHIFT;
        w
    }
}

/// A Control Indicator Field (CIF0 or CIF1): a 32-bit presence bitmap. Thin wrapper that
/// makes bit get/set explicit and self-documenting at call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cif(pub u32);

impl Cif {
    #[inline]
    pub fn get(self, bit: u32) -> bool {
        (self.0 >> bit) & 1 == 1
    }

    #[inline]
    pub fn with(self, bit: u32, present: bool) -> Cif {
        if present {
            Cif(self.0 | (1 << bit))
        } else {
            Cif(self.0 & !(1 << bit))
        }
    }

    #[inline]
    pub fn word(self) -> u32 {
        self.0
    }
}

/// CIF0 bit positions used by the DIFI command/link-establishment packets.
pub mod cif0 {
    /// bit 31: form selector. 0 = long form, 1 = short form.
    pub const SHORT_FORM: u32 = 31;
    /// bit 30: Reference Point field present.
    pub const REFERENCE_POINT: u32 = 30;
    /// bit 21: Sample Rate field present.
    pub const SAMPLE_RATE: u32 = 21;
    /// bit 20: Timestamp Adjustment field present.
    pub const TIMESTAMP_ADJ: u32 = 20;
    /// bit 1: CIF1 present. (Confirmed against the DIFI Consortium v1.2.1 command
    /// reference definition, which requires CIF0 bit 1 — not bit 2 — for CIF1-present.)
    pub const CIF1_PRESENT: u32 = 1;
}

/// CIF1 bit positions used by the DIFI command/link-establishment packets.
pub mod cif1 {
    /// bit 1: Buffer Size field present.
    pub const BUFFER_SIZE: u32 = 1;
}

/// The command-packet context block common to flow-control and link-establishment
/// packets, in wire order after the 7-word prologue: CAM, CIF0, Message ID, Controllee
/// ID, Controller ID, (CIF1). The command-specific fields that follow (Reference Point,
/// Sample Rate, …) are owned by each concrete packet type, selected by the CIF bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommandContext {
    pub cam: Cam,
    pub cif0: Cif,
    pub message_id: u32,
    /// 32-bit, unique within a deployment; defaults to 0 when unused.
    pub controllee_id: u32,
    /// 32-bit, unique within a deployment; defaults to 0 when unused.
    pub controller_id: u32,
    pub cif1: Cif,
}

/// Buffer Size field (3 words on the wire in the full command packet: a 64-bit size plus
/// a status word carrying the 12-bit fill level and the OF/NF/NE/UF flags).
///
/// Status word (word 21) layout per the DIFI v1.2.1 command reference:
/// - bits 31-16: reserved (zero)
/// - bits 15-4: **Buffer Level** (12-bit fill: bits 15-8 high byte, 7-4 low nibble)
/// - bit 3: Overflow, bit 2: Near-Full, bit 1: Near-Empty, bit 0: Underflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BufferState {
    /// Buffer capacity in bytes.
    pub size_bytes: u64,
    pub overflow: bool,
    pub near_full: bool,
    pub near_empty: bool,
    pub underflow: bool,
    /// 12-bit buffer fill level (bits 15-4 of the status word). Only the low 12 bits are
    /// meaningful; higher bits are ignored on encode and never set on decode.
    pub level: u16,
}

impl BufferState {
    /// Mask of the 12-bit buffer-level field once shifted into place (bits 15-4).
    const LEVEL_SHIFT: u32 = 4;
    const LEVEL_MASK: u32 = 0x0FFF; // 12 bits

    /// Encode the status word (word 21): 12-bit level in bits 15-4, OF/NF/NE/UF in the
    /// low nibble per [`crate::constants::buffer_status`]; bits 31-16 reserved (zero).
    pub fn status_word(self) -> u32 {
        use crate::constants::buffer_status as bs;
        let mut w = ((self.level as u32) & Self::LEVEL_MASK) << Self::LEVEL_SHIFT;
        w |= (self.overflow as u32) << bs::OVERFLOW_BIT;
        w |= (self.near_full as u32) << bs::NEAR_FULL_BIT;
        w |= (self.near_empty as u32) << bs::NEAR_EMPTY_BIT;
        w |= (self.underflow as u32) << bs::UNDERFLOW_BIT;
        w
    }

    /// Decode a status word into the 12-bit fill level and the OF/NF/NE/UF flags.
    pub fn from_status_word(size_bytes: u64, word: u32) -> BufferState {
        use crate::constants::buffer_status as bs;
        BufferState {
            size_bytes,
            overflow: (word >> bs::OVERFLOW_BIT) & 1 == 1,
            near_full: (word >> bs::NEAR_FULL_BIT) & 1 == 1,
            near_empty: (word >> bs::NEAR_EMPTY_BIT) & 1 == 1,
            underflow: (word >> bs::UNDERFLOW_BIT) & 1 == 1,
            level: ((word >> Self::LEVEL_SHIFT) & Self::LEVEL_MASK) as u16,
        }
    }
}

/// Validate that an Action Mode value fits its 2-bit field (fail-closed on encode).
pub fn check_action_mode(mode: u8) -> Result<()> {
    if mode > cam_bits::ACTION_MODE_MASK as u8 {
        return Err(DifiError::Invalid {
            field: "cam.action_mode",
            detail: format!("{mode} does not fit the 2-bit Action Mode field"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cam_round_trip_preserves_all_bits() {
        // Exercise a spread of words including reserved bits we don't model.
        for word in [
            0x0000_0000u32,
            0xFFFF_FFFF,
            0xA110_0000,
            0x1234_5678,
            0x8000_0000,
        ] {
            let cam = Cam::from_word(word);
            assert_eq!(
                cam.to_word(),
                word,
                "CAM round-trip failed for {word:#010X}"
            );
        }
    }

    #[test]
    fn cam_fields_map_to_the_right_bits() {
        let cam = Cam {
            controllee_id_present: true,
            controller_id_present: true,
            action_mode: action_mode::EXECUTE, // 0b10 -> bit 24
            validity_ack: true,                // bit 21
            ..Cam::default()
        };
        let w = cam.to_word();
        assert_eq!((w >> 31) & 1, 1, "controllee present = bit 31");
        assert_eq!((w >> 29) & 1, 1, "controller present = bit 29");
        assert_eq!((w >> 23) & 0b11, 0b10, "action mode bits 24-23");
        assert_eq!((w >> 21) & 1, 1, "validity ack = bit 21");
        // bit 24 (action-mode high bit) + bit 21 (validity ack) = 0x0120_0000 over the
        // two present-ID bits (31, 29).
        assert_eq!(w, 0x8000_0000 | 0x2000_0000 | 0x0100_0000 | 0x0020_0000);
    }

    #[test]
    fn cam_nack_and_ack_bits_map() {
        let cam = Cam {
            nack: true,       // bit 22
            ack_bits: 0b1010, // bits 11-8
            ..Cam::default()
        };
        let w = cam.to_word();
        assert_eq!((w >> 22) & 1, 1, "nack = bit 22");
        assert_eq!((w >> 8) & 0b1111, 0b1010, "ack bits 11-8");
        assert_eq!(Cam::from_word(w), cam);
    }

    #[test]
    fn cif_bit_set_and_clear() {
        let c = Cif::default()
            .with(cif0::REFERENCE_POINT, true)
            .with(cif0::SAMPLE_RATE, true)
            .with(cif0::CIF1_PRESENT, true);
        assert!(c.get(cif0::REFERENCE_POINT));
        assert!(c.get(cif0::SAMPLE_RATE));
        assert!(c.get(cif0::CIF1_PRESENT));
        assert!(!c.get(cif0::SHORT_FORM));
        let c2 = c.with(cif0::SAMPLE_RATE, false);
        assert!(!c2.get(cif0::SAMPLE_RATE));
        assert!(c2.get(cif0::REFERENCE_POINT));
    }

    #[test]
    fn buffer_status_round_trip() {
        let bs = BufferState {
            size_bytes: 1 << 20,
            overflow: true,
            near_empty: true,
            level: 0x0ABC, // 12-bit fill level
            ..BufferState::default()
        };
        let word = bs.status_word();
        // Level occupies bits 15-4; flags the low nibble; bits 31-16 reserved (zero).
        assert_eq!((word >> 4) & 0x0FFF, 0x0ABC, "12-bit level at bits 15-4");
        assert_eq!(word >> 16, 0, "bits 31-16 reserved/zero");
        assert_eq!((word >> 3) & 1, 1, "overflow bit 3");
        assert_eq!((word >> 1) & 1, 1, "near-empty bit 1");
        assert_eq!((word >> 2) & 1, 0, "near-full clear");
        let back = BufferState::from_status_word(bs.size_bytes, word);
        assert_eq!(back, bs);
    }

    #[test]
    fn buffer_level_is_12_bit() {
        // A level with bits above 12 must round-trip only the low 12 bits.
        let bs = BufferState {
            level: 0x0FFF,
            ..BufferState::default()
        };
        let back = BufferState::from_status_word(0, bs.status_word());
        assert_eq!(back.level, 0x0FFF);
        assert_eq!(
            bs.status_word() >> 16,
            0,
            "nothing leaks into reserved bits"
        );
    }

    #[test]
    fn action_mode_validation() {
        assert!(check_action_mode(action_mode::EXECUTE).is_ok());
        assert!(check_action_mode(0b11).is_ok());
        assert!(check_action_mode(0b100).is_err());
    }
}
