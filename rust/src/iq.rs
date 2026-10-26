// SPDX-License-Identifier: Apache-2.0
//! I/Q sample (un)packing for signed-integer bit depths 4..=16.
//!
//! DIFI carries complex Cartesian I/Q as signed integers with a selectable bit depth
//! (set in the Signal Context packet). Samples are packed with **no zero padding**:
//! for depths other than 8 and 16, samples share bytes.
//!
//! ## Packing convention (shared across all implementations)
//!
//! Each complex sample contributes two components in order: **I first, then Q**.
//! Every component is a two's-complement signed integer of `bit_depth` bits. Components
//! are written into a continuous bitstream **most-significant-bit first**, and the
//! bitstream is laid into the 32-bit payload words in big-endian byte order (i.e. the
//! first component occupies the most-significant bits of the first word). This matches
//! the DIFI "link-efficient" packing described in the primer.
//!
//! The caller guarantees word alignment: `samples * 2 * bit_depth` must be a whole
//! number of 32 bits. [`payload_word_count`] computes this and reports misalignment.

use crate::constants::{MAX_BIT_DEPTH, MIN_BIT_DEPTH};
use crate::error::{DifiError, Result};

/// A complex I/Q sample as signed integers in the chosen bit depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IqSample {
    pub i: i32,
    pub q: i32,
}

impl IqSample {
    pub fn new(i: i32, q: i32) -> Self {
        Self { i, q }
    }
}

/// Validate a bit depth against the DIFI-supported range.
pub fn check_bit_depth(bit_depth: u8) -> Result<()> {
    if (MIN_BIT_DEPTH..=MAX_BIT_DEPTH).contains(&bit_depth) {
        Ok(())
    } else {
        Err(DifiError::BadBitDepth { actual: bit_depth })
    }
}

/// Maximum magnitude representable: a `n`-bit signed sample spans
/// `-(2^(n-1)) ..= 2^(n-1) - 1`. The primer notes max magnitude `2^(n-1) - 1`.
pub fn sample_range(bit_depth: u8) -> (i32, i32) {
    let max = (1i32 << (bit_depth - 1)) - 1;
    let min = -(1i32 << (bit_depth - 1));
    (min, max)
}

/// Number of 32-bit payload words required for `sample_count` complex samples at
/// `bit_depth`. Fails if the result is not a whole number of words (Req 2.4).
pub fn payload_word_count(sample_count: usize, bit_depth: u8) -> Result<usize> {
    check_bit_depth(bit_depth)?;
    let total_bits = sample_count * 2 * bit_depth as usize;
    if total_bits % 32 != 0 {
        return Err(DifiError::NotWordAligned {
            field: "samples*2*bit_depth (bits)",
            value: total_bits,
        });
    }
    Ok(total_bits / 32)
}

/// Pack samples into a big-endian, 32-bit-word-aligned byte buffer, MSB-first.
pub fn pack_samples(samples: &[IqSample], bit_depth: u8) -> Result<Vec<u8>> {
    check_bit_depth(bit_depth)?;
    let (min, max) = sample_range(bit_depth);
    let words = payload_word_count(samples.len(), bit_depth)?;
    let mut out = vec![0u8; words * 4];

    let mut bit_pos: usize = 0;
    let mut write_component = |value: i32, out: &mut [u8]| -> Result<()> {
        if value < min || value > max {
            return Err(DifiError::SampleOutOfRange { value, bit_depth });
        }
        // Mask to the low `bit_depth` bits (two's-complement representation).
        let mask: u32 = if bit_depth == 32 {
            u32::MAX
        } else {
            (1u32 << bit_depth) - 1
        };
        let bits = (value as u32) & mask;
        // Write MSB-first.
        for k in (0..bit_depth as usize).rev() {
            let bit = (bits >> k) & 1;
            if bit != 0 {
                let byte = bit_pos / 8;
                let shift = 7 - (bit_pos % 8);
                out[byte] |= (bit as u8) << shift;
            }
            bit_pos += 1;
        }
        Ok(())
    };

    for s in samples {
        write_component(s.i, &mut out)?;
        write_component(s.q, &mut out)?;
    }
    Ok(out)
}

/// Unpack `sample_count` complex samples from a big-endian, MSB-first buffer.
pub fn unpack_samples(buf: &[u8], sample_count: usize, bit_depth: u8) -> Result<Vec<IqSample>> {
    check_bit_depth(bit_depth)?;
    let words = payload_word_count(sample_count, bit_depth)?;
    let need = words * 4;
    if buf.len() < need {
        return Err(DifiError::Truncated {
            field: "iq payload",
            need,
            have: buf.len(),
        });
    }

    let mut bit_pos: usize = 0;
    let sign_bit = 1u32 << (bit_depth - 1);
    let mut read_component = || -> i32 {
        let mut bits: u32 = 0;
        for _ in 0..bit_depth {
            let byte = bit_pos / 8;
            let shift = 7 - (bit_pos % 8);
            let bit = (buf[byte] >> shift) & 1;
            bits = (bits << 1) | bit as u32;
            bit_pos += 1;
        }
        // Sign-extend from `bit_depth` bits.
        if bits & sign_bit != 0 {
            (bits | !((1u32 << bit_depth) - 1)) as i32
        } else {
            bits as i32
        }
    };

    let mut out = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        let i = read_component();
        let q = read_component();
        out.push(IqSample::new(i, q));
    }
    Ok(out)
}
