// SPDX-License-Identifier: Apache-2.0
//! End-to-end export/import tests (spec task 12 / Req 7.1): a reference signal mapped
//! to DIFI integer, encoded, and round-tripped recovers within the documented
//! quantization loss, with transport/codec adding no error of their own.

use difi::{
    difi_to_float, float_to_difi, generate_signal, measure_implementation_loss,
    ScalingConvention, SignalParams,
};

#[test]
fn generator_is_deterministic() {
    let p = SignalParams::default();
    let a = generate_signal(&p);
    let b = generate_signal(&p);
    assert_eq!(a, b, "generator must be reproducible");
    assert_eq!(a.len(), p.num_samples);
    // Peak magnitude must respect the requested peak (<= 1.0 for full-scale mapping).
    let max_mag = a
        .iter()
        .map(|&(i, q)| ((i as f64).powi(2) + (q as f64).powi(2)).sqrt())
        .fold(0.0f64, f64::max);
    assert!(max_mag <= p.peak as f64 + 1e-6, "peak {max_mag} exceeds {}", p.peak);
    assert!(max_mag > 0.8 * p.peak as f64, "peak {max_mag} unexpectedly small");
}

#[test]
fn export_round_trip_within_documented_loss() {
    // This is the in-process equivalent of the UDP export/import: generate -> map ->
    // (encode/transport/decode is exercised by the transport tests) -> inverse-map.
    // Here we assert the signal-level property: recovered == quantised original, with
    // loss bounded by one LSB and no worse than the mapper-only loss.
    let params = SignalParams::default();
    let original = generate_signal(&params);
    let conv = ScalingConvention::new(16, 1.0).unwrap();

    let codes = float_to_difi(&original, &conv);
    let recovered = difi_to_float(&codes, &conv);

    let mapper = measure_implementation_loss(&original, &conv);
    let one_lsb = 1.0 / conv.fs_code() as f64;

    let mut max_abs = 0.0f64;
    for (&(oi, oq), &(ri, rq)) in original.iter().zip(recovered.iter()) {
        max_abs = max_abs.max(((ri - oi) as f64).abs());
        max_abs = max_abs.max(((rq - oq) as f64).abs());
    }
    assert_eq!(recovered.len(), original.len());
    assert!(max_abs <= one_lsb, "max error {max_abs} exceeds 1 LSB {one_lsb}");
    assert!(
        (max_abs - mapper.max_abs_error).abs() < 1e-9,
        "end-to-end loss {max_abs} must equal mapper-only loss {}",
        mapper.max_abs_error
    );
    assert!(mapper.snr_db > 70.0, "SNR {} dB too low for 16-bit", mapper.snr_db);
}
