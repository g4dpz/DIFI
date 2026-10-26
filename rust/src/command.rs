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
/// Bit layout (MSB = bit 31):
/// - bit 31: Controllee ID Identifier present
/// - bit 30: Controllee ID format (0 = 32-bit)
/// - bit 29: Controller ID Identifier present
/// - bit 28: Controller ID format (0 = 32-bit)
/// - bits 24-23: Action Mode (0b10 = execute)
/// - bits 20-18: Acknowledgement type (0b100 = Validation Acknowledge)
/// - bits 14-12: Timestamp Control Mode
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
    /// Acknowledgement type, bits 20-18 (0b100 = Validation Acknowledge).
    pub ack_type: u8,
    /// Timestamp Control Mode, bits 14-12.
    pub timestamp_control: u8,
    /// Any bits the DIFI profile does not model, preserved verbatim for exact round-trip.
    pub reserved: u32,
}

// CAM sub-field bit positions / masks.
mod cam_bits {
    pub const CONTROLLEE_PRESENT: u32 = 31;
    pub const CONTROLLEE_FORMAT: u32 = 30;
    pub const CONTROLLER_PRESENT: u32 = 29;
    pub const CONTROLLER_FORMAT: u32 = 28;
    pub const ACTION_MODE_SHIFT: u32 = 23; // bits 24-23
    pub const ACTION_MODE_MASK: u32 = 0b11;
    pub const ACK_TYPE_SHIFT: u32 = 18; // bits 20-18
    pub const ACK_TYPE_MASK: u32 = 0b111;
    pub const TS_CONTROL_SHIFT: u32 = 12; // bits 14-12
    pub const TS_CONTROL_MASK: u32 = 0b111;

    /// All bits this module interprets; everything else is `reserved`.
    pub const MODELLED_MASK: u32 = (1 << CONTROLLEE_PRESENT)
        | (1 << CONTROLLEE_FORMAT)
        | (1 << CONTROLLER_PRESENT)
        | (1 << CONTROLLER_FORMAT)
        | (ACTION_MODE_MASK << ACTION_MODE_SHIFT)
        | (ACK_TYPE_MASK << ACK_TYPE_SHIFT)
        | (TS_CONTROL_MASK << TS_CONTROL_SHIFT);
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

/// Acknowledgement type values (CAM bits 20-18).
pub mod ack_type {
    /// No acknowledgement requested.
    pub const NONE: u8 = 0b000;
    /// Validation Acknowledgement (used by the Sink Capabilities Query).
    pub const VALIDATION: u8 = 0b100;
}

impl Cam {
    /// Decode a CAM word into its fields, preserving any unmodelled bits in `reserved`.
    pub fn from_word(word: u32) -> Cam {
        Cam {
            controllee_id_present: (word >> cam_bits::CONTROLLEE_PRESENT) & 1 == 1,
            controllee_id_format: (word >> cam_bits::CONTROLLEE_FORMAT) & 1 == 1,
            controller_id_present: (word >> cam_bits::CONTROLLER_PRESENT) & 1 == 1,
            controller_id_format: (word >> cam_bits::CONTROLLER_FORMAT) & 1 == 1,
            action_mode: ((word >> cam_bits::ACTION_MODE_SHIFT) & cam_bits::ACTION_MODE_MASK)
                as u8,
            ack_type: ((word >> cam_bits::ACK_TYPE_SHIFT) & cam_bits::ACK_TYPE_MASK) as u8,
            timestamp_control: ((word >> cam_bits::TS_CONTROL_SHIFT) & cam_bits::TS_CONTROL_MASK)
                as u8,
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
        w |= ((self.action_mode as u32) & cam_bits::ACTION_MODE_MASK) << cam_bits::ACTION_MODE_SHIFT;
        w |= ((self.ack_type as u32) & cam_bits::ACK_TYPE_MASK) << cam_bits::ACK_TYPE_SHIFT;
        w |= ((self.timestamp_control as u32) & cam_bits::TS_CONTROL_MASK)
            << cam_bits::TS_CONTROL_SHIFT;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl Default for CommandContext {
    fn default() -> Self {
        CommandContext {
            cam: Cam::default(),
            cif0: Cif::default(),
            message_id: 0,
            controllee_id: 0,
            controller_id: 0,
            cif1: Cif::default(),
        }
    }
}

/// Buffer Size field (3 words on the wire in the full command packet: a 64-bit size plus
/// a status word carrying the OF/NF/NE/UF fill-level flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BufferState {
    /// Buffer capacity in bytes.
    pub size_bytes: u64,
    pub overflow: bool,
    pub near_full: bool,
    pub near_empty: bool,
    pub underflow: bool,
    /// Buffer level value (the field above the status nibble); kept for round-trip.
    pub level: u32,
}

impl BufferState {
    /// Encode the status word (word 21): level in the high bits, OF/NF/NE/UF in the low
    /// nibble per [`crate::constants::buffer_status`].
    pub fn status_word(self) -> u32 {
        use crate::constants::buffer_status as bs;
        let mut w = self.level & !0xF; // keep level, clear the flag nibble
        w |= (self.overflow as u32) << bs::OVERFLOW_BIT;
        w |= (self.near_full as u32) << bs::NEAR_FULL_BIT;
        w |= (self.near_empty as u32) << bs::NEAR_EMPTY_BIT;
        w |= (self.underflow as u32) << bs::UNDERFLOW_BIT;
        w
    }

    /// Decode a status word into the fill-level flags and preserved level bits.
    pub fn from_status_word(size_bytes: u64, word: u32) -> BufferState {
        use crate::constants::buffer_status as bs;
        BufferState {
            size_bytes,
            overflow: (word >> bs::OVERFLOW_BIT) & 1 == 1,
            near_full: (word >> bs::NEAR_FULL_BIT) & 1 == 1,
            near_empty: (word >> bs::NEAR_EMPTY_BIT) & 1 == 1,
            underflow: (word >> bs::UNDERFLOW_BIT) & 1 == 1,
            level: word & !0xF,
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
        for word in [0x0000_0000u32, 0xFFFF_FFFF, 0xA110_0000, 0x1234_5678, 0x8000_0000] {
            let cam = Cam::from_word(word);
            assert_eq!(cam.to_word(), word, "CAM round-trip failed for {word:#010X}");
        }
    }

    #[test]
    fn cam_fields_map_to_the_right_bits() {
        let cam = Cam {
            controllee_id_present: true,
            controller_id_present: true,
            action_mode: action_mode::EXECUTE, // 0b10 -> bit 24
            ack_type: ack_type::VALIDATION,    // 0b100 -> bit 20
            ..Cam::default()
        };
        let w = cam.to_word();
        assert_eq!((w >> 31) & 1, 1, "controllee present = bit 31");
        assert_eq!((w >> 29) & 1, 1, "controller present = bit 29");
        assert_eq!((w >> 23) & 0b11, 0b10, "action mode bits 24-23");
        assert_eq!((w >> 18) & 0b111, 0b100, "ack type bits 20-18");
        // bit 24 set (action mode high bit), bit 20 set (ack type high bit)
        assert_eq!(w, 0x8000_0000 | 0x2000_0000 | 0x0100_0000 | 0x0010_0000);
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
            level: 0x1234_5670, // low nibble is the flags, high bits are level
            ..BufferState::default()
        };
        let word = bs.status_word();
        assert_eq!((word >> 3) & 1, 1, "overflow bit 3");
        assert_eq!((word >> 1) & 1, 1, "near-empty bit 1");
        assert_eq!((word >> 2) & 1, 0, "near-full clear");
        let back = BufferState::from_status_word(bs.size_bytes, word);
        assert_eq!(back, bs);
    }

    #[test]
    fn action_mode_validation() {
        assert!(check_action_mode(action_mode::EXECUTE).is_ok());
        assert!(check_action_mode(0b11).is_ok());
        assert!(check_action_mode(0b100).is_err());
    }
}
