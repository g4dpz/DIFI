// SPDX-License-Identifier: Apache-2.0
//! Sample-mapper tests (spec task 8 / Req 4): deterministic float↔integer mapping and
//! measured round-trip loss.

use difi::iq::sample_range;
use difi::{
    difi_to_float, float_to_difi, measure_implementation_loss, IqSample, ScalingConvention,
};

#[test]
fn full_scale_and_zero_map_exactly() {
    for &depth in &[4u8, 8, 12, 16] {
        let conv = ScalingConvention::new(depth, 1.0).unwrap();
        let fs = sample_range(depth).1;
        assert_eq!(conv.float_to_code(0.0), 0);
        assert_eq!(conv.float_to_code(1.0), fs);
        assert_eq!(conv.float_to_code(-1.0), -fs);
    }
}

#[test]
fn clamps_overrange_rather_than_wrapping() {
    // A sample above +full_scale must saturate to +FS, not wrap to a negative code.
    let conv = ScalingConvention::new(8, 1.0).unwrap();
    let (min, max) = sample_range(8);
    assert_eq!(conv.float_to_code(5.0), max);
    assert_eq!(conv.float_to_code(-5.0), min.max(-max)); // -FS, within [min,max]
    assert_eq!(conv.float_to_code(5.0), 127);
    assert_eq!(conv.float_to_code(-5.0), -127);
}

#[test]
fn round_trip_loss_is_bounded_and_measured() {
    // A smooth ramp across the dynamic range. Round-trip quantisation error must be
    // at most ~1 LSB per component, and SNR must be finite and reasonable.
    let depth = 12u8;
    let conv = ScalingConvention::new(depth, 1.0).unwrap();
    let fs = sample_range(depth).1 as f64;
    let one_lsb = (conv.full_scale / fs) as f64;

    let n = 1024usize;
    let iq: Vec<(f32, f32)> = (0..n)
        .map(|k| {
            let phase = (k as f64) / (n as f64) * std::f64::consts::TAU;
            (phase.cos() as f32, phase.sin() as f32)
        })
        .collect();

    let report = measure_implementation_loss(&iq, &conv);
    assert_eq!(report.sample_count, n);
    // Max error must not exceed half an LSB of rounding + tiny float slack.
    assert!(
        report.max_abs_error <= one_lsb * 0.75,
        "max_abs_error {} exceeded ~0.75 LSB ({})",
        report.max_abs_error,
        one_lsb
    );
    assert!(report.rms_error > 0.0, "quantisation should not be lossless");
    // A 12-bit full-scale sine should land well above 60 dB SNR.
    assert!(report.snr_db > 60.0, "SNR {} dB too low", report.snr_db);
}

#[test]
fn custom_full_scale_round_trips_within_tolerance() {
    // full_scale != 1.0: a signal whose amplitude reference is, say, 2.5 V.
    let conv = ScalingConvention::new(16, 2.5).unwrap();
    let iq = vec![(1.25f32, -0.625f32), (2.5, -2.5), (0.0, 0.0)];
    let codes = float_to_difi(&iq, &conv);
    let back = difi_to_float(&codes, &conv);
    for (&(oi, oq), &(ri, rq)) in iq.iter().zip(back.iter()) {
        // Within one LSB in the 2.5-scaled domain.
        let lsb = 2.5 / (sample_range(16).1 as f32);
        assert!((ri - oi).abs() <= lsb, "I {ri} vs {oi}");
        assert!((rq - oq).abs() <= lsb, "Q {rq} vs {oq}");
    }
    // Full-scale maps to the max code.
    assert_eq!(codes[1], IqSample::new(sample_range(16).1, -sample_range(16).1));
}

#[test]
fn rejects_bad_bit_depth() {
    assert!(ScalingConvention::new(3, 1.0).is_err());
    assert!(ScalingConvention::new(17, 1.0).is_err());
}
