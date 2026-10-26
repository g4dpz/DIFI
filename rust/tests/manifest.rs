// SPDX-License-Identifier: Apache-2.0
//! Signal manifest tests (spec task 11 / Req 6.1, 6.2): the generator is reproducible
//! and the manifest's SHA-256 matches a fresh regeneration.

use difi::manifest::encode_cf32;
use difi::{build_manifest, generate_signal, DifiContext, SignalParams};

fn difi_ctx() -> DifiContext {
    DifiContext {
        bit_depth: 16,
        full_scale: 1.0,
        rf_reference_hz: 2.401e9,
        bandwidth_hz: 1.25e6,
        sample_rate_hz: 1.25e6,
        gain_db: -3.5,
    }
}

#[test]
fn regenerate_matches_manifest_sha256() {
    let params = SignalParams::default();

    // First generation -> manifest.
    let raw1 = encode_cf32(&generate_signal(&params));
    let m = build_manifest("ref", &raw1, params, difi_ctx(), "difi-tool generate");

    // Regenerate from the same params and confirm the SHA-256 matches (Req 6.2 test).
    let raw2 = encode_cf32(&generate_signal(&params));
    assert_eq!(raw1, raw2, "generator must be byte-reproducible");
    assert_eq!(difi::sha256::hex_digest(&raw2), m.sha256);
    assert_eq!(raw2.len(), m.byte_size);
}

#[test]
fn manifest_json_records_required_fields() {
    let params = SignalParams::default();
    let raw = encode_cf32(&generate_signal(&params));
    let m = build_manifest("afs-style", &raw, params, difi_ctx(), "difi-tool generate work");
    let json = m.to_json();

    // Req 6.2: SHA-256, size, sample format, DIFI context params, regen command.
    assert!(json.contains("\"sha256\""));
    assert!(json.contains(&format!("\"byte_size\": {}", raw.len())));
    assert!(json.contains("\"raw_format\": \"cf32\""));
    assert!(json.contains("\"bit_depth\": 16"));
    assert!(json.contains("\"rf_reference_hz\": 2401000000"));
    assert!(json.contains("\"regen_command\""));
    assert!(json.contains("\"num_samples\": 4096"));
}

#[test]
fn cf32_encoding_is_little_endian_float32() {
    let iq = vec![(1.0f32, -1.0f32)];
    let bytes = encode_cf32(&iq);
    assert_eq!(bytes.len(), 8);
    // 1.0f32 = 0x3F800000 little-endian = 00 00 80 3F
    assert_eq!(&bytes[0..4], &[0x00, 0x00, 0x80, 0x3F]);
    // -1.0f32 = 0xBF800000 little-endian = 00 00 80 BF
    assert_eq!(&bytes[4..8], &[0x00, 0x00, 0x80, 0xBF]);
}
