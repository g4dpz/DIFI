// SPDX-License-Identifier: Apache-2.0
//! Sample Mapper — float ↔ DIFI signed-integer conversion (spec task 8 / Req 4).
//!
//! DIFI carries signed-integer I/Q; existing reference signals (e.g. the competition
//! LSIS-AFS vectors) are float32. This module pins the **scaling / reference-level
//! convention** that maps between them, deterministically and identically across every
//! implementation in the toolkit.
//!
//! ## Convention (shared, documented — DQ-2)
//!
//! A float sample is interpreted as a normalised amplitude where **`full_scale`
//! corresponds to the largest-magnitude integer code**. For a bit depth `n` the signed
//! range is `-(2^(n-1)) ..= 2^(n-1) - 1`; we use `FS = 2^(n-1) - 1` (the positive
//! max-magnitude code, per the primer) as the full-scale reference, applied symmetrically
//! to I and Q.
//!
//! - **float → int:** `code = round(clamp(x / full_scale, -1, 1) * FS)`.
//!   Values beyond ±`full_scale` are **clamped** (saturating), never wrapped — a
//!   loud sample saturates rather than aliasing to the opposite sign.
//! - **int → float:** `x = code / FS * full_scale`.
//! - Rounding is round-half-away-from-zero (symmetric), so +0.5 LSB → +1 and −0.5 LSB
//!   → −1, keeping I and Q symmetric.
//!
//! `full_scale` is the real-world amplitude that maps to 0 dBFS and is recorded in the
//! signal manifest (Req 4.3). With `full_scale = 1.0` the mapping is the common
//! normalised-to-±1.0 convention.
//!
//! Round-trip is **not** lossless (quantisation); [`measure_implementation_loss`]
//! reports the actual error so callers never assume zero (Req 4.2).

use crate::error::Result;
use crate::iq::{check_bit_depth, sample_range, IqSample};

/// Scaling convention for a float↔integer mapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalingConvention {
    /// Signed-integer bit depth (4..=16).
    pub bit_depth: u8,
    /// Real-world amplitude mapped to full scale (0 dBFS). `1.0` = normalised ±1.0.
    pub full_scale: f64,
}

impl ScalingConvention {
    pub fn new(bit_depth: u8, full_scale: f64) -> Result<Self> {
        check_bit_depth(bit_depth)?;
        Ok(Self {
            bit_depth,
            full_scale,
        })
    }

    /// Positive full-scale integer code `FS = 2^(n-1) - 1`.
    pub fn fs_code(&self) -> i32 {
        sample_range(self.bit_depth).1
    }

    /// Map one float component to a clamped, rounded signed-integer code.
    pub fn float_to_code(&self, x: f32) -> i32 {
        let (min, max) = sample_range(self.bit_depth);
        let fs = self.fs_code() as f64;
        let norm = (x as f64 / self.full_scale).clamp(-1.0, 1.0);
        let scaled = norm * fs;
        // Round half away from zero for symmetry.
        let rounded = if scaled >= 0.0 {
            (scaled + 0.5).floor()
        } else {
            (scaled - 0.5).ceil()
        } as i32;
        rounded.clamp(min, max)
    }

    /// Map one signed-integer code back to a float component.
    pub fn code_to_float(&self, code: i32) -> f32 {
        let fs = self.fs_code() as f64;
        ((code as f64 / fs) * self.full_scale) as f32
    }
}

/// Convert float32 complex I/Q to DIFI integer samples.
pub fn float_to_difi(iq: &[(f32, f32)], conv: &ScalingConvention) -> Vec<IqSample> {
    iq.iter()
        .map(|&(i, q)| IqSample::new(conv.float_to_code(i), conv.float_to_code(q)))
        .collect()
}

/// Convert DIFI integer samples back to float32 complex I/Q.
pub fn difi_to_float(samples: &[IqSample], conv: &ScalingConvention) -> Vec<(f32, f32)> {
    samples
        .iter()
        .map(|s| (conv.code_to_float(s.i), conv.code_to_float(s.q)))
        .collect()
}

/// Round-trip implementation-loss report (Req 4.2). All errors are measured, not assumed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LossReport {
    /// Number of complex samples measured.
    pub sample_count: usize,
    /// Maximum absolute per-component error (float units).
    pub max_abs_error: f64,
    /// RMS error across all I and Q components (float units).
    pub rms_error: f64,
    /// Signal-to-quantisation-noise ratio in dB (higher is better). `inf` if error is 0.
    pub snr_db: f64,
}

/// Measure the loss of a float → DIFI-integer → float round trip at this convention.
pub fn measure_implementation_loss(iq: &[(f32, f32)], conv: &ScalingConvention) -> LossReport {
    let codes = float_to_difi(iq, conv);
    let recovered = difi_to_float(&codes, conv);

    let mut sum_sq_err = 0.0f64;
    let mut sum_sq_sig = 0.0f64;
    let mut max_abs = 0.0f64;
    let n_components = iq.len() * 2;

    for (&(oi, oq), &(ri, rq)) in iq.iter().zip(recovered.iter()) {
        for (orig, rec) in [(oi, ri), (oq, rq)] {
            let e = (rec as f64) - (orig as f64);
            sum_sq_err += e * e;
            sum_sq_sig += (orig as f64) * (orig as f64);
            max_abs = max_abs.max(e.abs());
        }
    }

    let rms_error = if n_components > 0 {
        (sum_sq_err / n_components as f64).sqrt()
    } else {
        0.0
    };
    let snr_db = if sum_sq_err == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (sum_sq_sig / sum_sq_err).log10()
    };

    LossReport {
        sample_count: iq.len(),
        max_abs_error: max_abs,
        rms_error,
        snr_db,
    }
}
