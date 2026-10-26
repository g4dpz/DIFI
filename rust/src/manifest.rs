//! Signal manifest writer (spec task 11 / Req 6.2, 6.4).
//!
//! A manifest records everything needed to reproduce and verify a reference signal
//! without committing the raw I/Q: the exact generator parameters, the byte size and
//! SHA-256 of the generated output, the sample format (raw float32 + the DIFI integer
//! mapping it is intended for), the DIFI context parameters, and the exact regeneration
//! command. The manifest is small and committed; the raw signal is not (artifact
//! policy — it lives in a gitignored working dir, regenerated on demand).
//!
//! The manifest is emitted as JSON, hand-serialised to keep the crate dependency-free.

use crate::signal::SignalParams;

/// Byte size of one `cf32` complex sample (two little-endian float32 components).
pub const CF32_SAMPLE_BYTES: usize = 8;

/// DIFI context parameters recorded alongside the signal (Req 6.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DifiContext {
    pub bit_depth: u8,
    pub full_scale: f64,
    pub rf_reference_hz: f64,
    pub bandwidth_hz: f64,
    pub sample_rate_hz: f64,
    pub gain_db: f64,
}

/// A complete signal manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    /// Human-readable signal name (also the output file stem).
    pub name: String,
    /// Raw sample format of the generated file (e.g. "cf32" = interleaved LE float32 I/Q).
    pub raw_format: String,
    /// Byte size of the generated raw file.
    pub byte_size: usize,
    /// SHA-256 (lowercase hex) of the generated raw file.
    pub sha256: String,
    /// The exact generator parameters that reproduce the signal.
    pub params: SignalParams,
    /// The DIFI integer mapping / context parameters the signal is exported with.
    pub difi: DifiContext,
    /// The exact command that regenerates the raw file.
    pub regen_command: String,
}

impl Manifest {
    /// Serialise to pretty JSON (hand-written, stable field order).
    pub fn to_json(&self) -> String {
        let p = &self.params;
        let d = &self.difi;
        format!(
            "{{\n  \"name\": \"{name}\",\n  \"raw_format\": \"{raw}\",\n  \"byte_size\": {size},\n  \"sha256\": \"{sha}\",\n  \"params\": {{\n    \"num_samples\": {ns},\n    \"sample_rate_hz\": {sr},\n    \"tone1_hz\": {t1},\n    \"tone2_hz\": {t2},\n    \"peak\": {peak}\n  }},\n  \"difi\": {{\n    \"bit_depth\": {bd},\n    \"full_scale\": {fsc},\n    \"rf_reference_hz\": {rf},\n    \"bandwidth_hz\": {bw},\n    \"sample_rate_hz\": {dsr},\n    \"gain_db\": {gain}\n  }},\n  \"regen_command\": \"{cmd}\"\n}}\n",
            name = self.name,
            raw = self.raw_format,
            size = self.byte_size,
            sha = self.sha256,
            ns = p.num_samples,
            sr = fmt_f64(p.sample_rate_hz),
            t1 = fmt_f64(p.tone1_hz),
            t2 = fmt_f64(p.tone2_hz),
            peak = fmt_f32(p.peak),
            bd = d.bit_depth,
            fsc = fmt_f64(d.full_scale),
            rf = fmt_f64(d.rf_reference_hz),
            bw = fmt_f64(d.bandwidth_hz),
            dsr = fmt_f64(d.sample_rate_hz),
            gain = fmt_f64(d.gain_db),
            cmd = self.regen_command,
        )
    }
}

/// Encode a float32 complex signal to `cf32` bytes: interleaved I, Q, each a
/// little-endian IEEE-754 float32. This is the common SDR complex-float format.
pub fn encode_cf32(iq: &[(f32, f32)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(iq.len() * CF32_SAMPLE_BYTES);
    for &(i, q) in iq {
        out.extend_from_slice(&i.to_le_bytes());
        out.extend_from_slice(&q.to_le_bytes());
    }
    out
}

/// Build a manifest for a signal generated from `params` and exported with `difi`.
/// `raw_bytes` is the generated `cf32` file content (used for size + SHA-256).
pub fn build(
    name: &str,
    raw_bytes: &[u8],
    params: SignalParams,
    difi: DifiContext,
    regen_command: &str,
) -> Manifest {
    Manifest {
        name: name.to_string(),
        raw_format: "cf32".to_string(),
        byte_size: raw_bytes.len(),
        sha256: crate::sha256::hex_digest(raw_bytes),
        params,
        difi,
        regen_command: regen_command.to_string(),
    }
}

// Format floats without a trailing-zero mess, but stable across Rust and (for our
// purposes) readable. Integers-valued floats print without a decimal point via {}.
fn fmt_f64(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}
fn fmt_f32(v: f32) -> String {
    format!("{v}")
}
