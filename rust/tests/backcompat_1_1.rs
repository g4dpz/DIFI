// SPDX-License-Identifier: Apache-2.0
//! DIFI 1.1 backward-compatibility regression gate — spec `difi-1.2-dataplane` task 5.
//!
//! Decodes the committed 1.1 golden packets (the shared wire-format oracle) through the
//! decoder *as extended by the 1.2 additions* and asserts every documented field is
//! recovered exactly. This proves the relaxed class checks (task 4) and the extended
//! dispatch (tasks 3-4) did NOT change 1.1 decode behaviour.
//!
//! The encode-side byte-identity is covered separately: both tools regenerate the golden
//! packets and the C++ `golden_match` ctest / interop.sh §1 assert byte equality against
//! the committed oracle. This file covers the decode side.

use std::path::PathBuf;

use difi::constants::*;
use difi::{Decoder, DifiPacket, IqSample};

fn golden_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/rust ; the golden oracle is <repo>/tests/golden.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("golden")
}

fn read_golden(name: &str) -> Vec<u8> {
    let path = golden_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

#[test]
fn golden_signal_context_decodes_to_documented_fields() {
    let buf = read_golden("signal_context_16bit.difi.bin");
    assert_eq!(buf.len(), 108, "committed signal context is 27 words");
    let mut dec = Decoder::new();
    match dec.decode(&buf).expect("decode golden signal context") {
        DifiPacket::SignalContext(c) => {
            // Common header values (MANIFEST).
            assert_eq!(c.header.stream_id, 0x1234_5678);
            assert_eq!(c.header.integer_seconds, 1_700_000_000);
            assert_eq!(c.header.fractional_seconds, 123_456_789_000);
            // Class / type unchanged for 1.1 standard flow.
            assert_eq!(c.header.packet_type, packet_type::CONTEXT);
            assert_eq!(c.header.packet_class, packet_class::SIGNAL_CONTEXT);
            assert_eq!(c.header.tsf, tsf::REAL_TIME_PS);
            // Documented field values.
            assert!(c.context_changed);
            assert!((c.rf_reference_hz - 2.401e9).abs() < 1.0);
            assert!((c.bandwidth_hz - 1.0e6).abs() < 1.0);
            assert!((c.sample_rate_hz - 1.25e6).abs() < 1.0);
            assert!((c.gain_db - (-3.5)).abs() < 1e-6);
            assert_eq!(c.data_format.bit_depth, 16);
        }
        other => panic!("expected SignalContext, got {other:?}"),
    }
}

#[test]
fn golden_version_context_decodes_to_documented_fields() {
    let buf = read_golden("version_context.difi.bin");
    assert_eq!(buf.len(), 44, "committed version context is 11 words");
    let mut dec = Decoder::new();
    match dec.decode(&buf).expect("decode golden version context") {
        DifiPacket::VersionContext(v) => {
            assert_eq!(v.header.stream_id, 0x1234_5678);
            assert_eq!(v.header.packet_type, version_packet_type::VERSION_CONTEXT);
            assert_eq!(v.header.packet_class, packet_class::VERSION_CONTEXT);
            assert!(v.context_changed);
            assert_eq!(v.year, 2021);
            assert_eq!(v.day, 100);
        }
        other => panic!("expected VersionContext, got {other:?}"),
    }
}

#[test]
fn golden_signal_data_decodes_to_documented_samples() {
    let buf = read_golden("signal_data_16bit.difi.bin");
    assert_eq!(buf.len(), 60);
    let mut dec = Decoder::new();
    // Bit depth is out of band for Signal Data (comes from the context); the manifest
    // documents 16-bit. Seed it, exactly as a real receiver would from the context.
    dec.set_bit_depth(0x1234_5678, 16);
    match dec.decode(&buf).expect("decode golden signal data") {
        DifiPacket::SignalData(d) => {
            assert_eq!(d.header.stream_id, 0x1234_5678);
            assert_eq!(d.header.packet_type, packet_type::SIGNAL_DATA);
            assert_eq!(d.header.packet_class, packet_class::SIGNAL_DATA);
            assert_eq!(d.header.seq_num, 0);
            assert_eq!(d.bit_depth, 16);
            let expected = vec![
                IqSample { i: -4, q: 100 },
                IqSample { i: -3, q: 75 },
                IqSample { i: -2, q: 50 },
                IqSample { i: -1, q: 25 },
                IqSample { i: 0, q: 0 },
                IqSample { i: 1, q: -25 },
                IqSample { i: 2, q: -50 },
                IqSample { i: 3, q: -75 },
            ];
            assert_eq!(d.samples, expected);
        }
        other => panic!("expected SignalData, got {other:?}"),
    }
}

/// The 1.2 additions must not have reclassified any 1.1 golden packet. In particular, a
/// Signal Context golden must still decode as a standard-flow context (class 0x0001),
/// never as the sample-count twin (0x0003), and a Signal Data golden as 0x0000.
#[test]
fn golden_packets_keep_their_1_1_classes() {
    let mut dec = Decoder::new();
    dec.set_bit_depth(0x1234_5678, 16);
    let ctx = dec.decode(&read_golden("signal_context_16bit.difi.bin")).unwrap();
    assert!(matches!(ctx, DifiPacket::SignalContext(ref c)
        if c.header.packet_class == packet_class::SIGNAL_CONTEXT));
    let data = dec.decode(&read_golden("signal_data_16bit.difi.bin")).unwrap();
    assert!(matches!(data, DifiPacket::SignalData(ref d)
        if d.header.packet_class == packet_class::SIGNAL_DATA));
}
