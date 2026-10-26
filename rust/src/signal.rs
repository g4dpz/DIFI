//! Deterministic reference-signal generator (spec task 11 seed / Req 6.1).
//!
//! Large raw I/Q is never committed (artifact policy); instead a signal is reproduced
//! from a small set of committed parameters. This generator produces a float32 complex
//! baseband signal identically across implementations, so the same bytes can be
//! regenerated on demand and exchanged over DIFI (task 12 / Req 7.1).
//!
//! The waveform is a representative stand-in for an LSIS-AFS-style float32 reference:
//! a complex tone sum (two carriers) with a slow amplitude envelope, normalised so the
//! peak magnitude is `peak` (≤ 1.0 for the default full-scale mapping). It is not a real
//! LSIS-AFS vector — it is a reproducible waveform with the same float32 shape, suitable
//! for interop evidence until a real vector's generator parameters are committed.

use std::f64::consts::TAU;

/// Parameters that deterministically reproduce a reference signal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalParams {
    /// Number of complex samples.
    pub num_samples: usize,
    /// Sample rate in Hz (sets the normalised tone frequencies).
    pub sample_rate_hz: f64,
    /// First tone frequency in Hz.
    pub tone1_hz: f64,
    /// Second tone frequency in Hz.
    pub tone2_hz: f64,
    /// Peak magnitude of the generated signal (normalised amplitude).
    pub peak: f32,
}

impl Default for SignalParams {
    fn default() -> Self {
        // Committed reference parameters. Changing these changes the signal identity.
        Self {
            num_samples: 4096,
            sample_rate_hz: 1.25e6,
            tone1_hz: 50_000.0,
            tone2_hz: -120_000.0,
            peak: 0.9,
        }
    }
}

/// Generate the reference float32 I/Q signal from parameters, deterministically.
pub fn generate(params: &SignalParams) -> Vec<(f32, f32)> {
    let fs = params.sample_rate_hz;
    let w1 = TAU * params.tone1_hz / fs;
    let w2 = TAU * params.tone2_hz / fs;
    let n = params.num_samples;

    // First pass: raw complex sum with a slow amplitude envelope.
    let mut raw: Vec<(f64, f64)> = Vec::with_capacity(n);
    let mut max_mag = 0.0f64;
    for k in 0..n {
        let t = k as f64;
        // Slow envelope between 0.5 and 1.0 so amplitude varies across the capture.
        let env = 0.75 + 0.25 * (TAU * (k as f64) / (n as f64)).cos();
        let re = env * ((w1 * t).cos() + 0.6 * (w2 * t).cos());
        let im = env * ((w1 * t).sin() + 0.6 * (w2 * t).sin());
        let mag = (re * re + im * im).sqrt();
        if mag > max_mag {
            max_mag = mag;
        }
        raw.push((re, im));
    }

    // Normalise so the peak magnitude equals `peak`.
    let scale = if max_mag > 0.0 {
        params.peak as f64 / max_mag
    } else {
        0.0
    };
    raw.into_iter()
        .map(|(re, im)| ((re * scale) as f32, (im * scale) as f32))
        .collect()
}
