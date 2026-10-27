// SPDX-License-Identifier: Apache-2.0
//! DIFI 1.2 Sample Count Data/Context twins (`0x0002`/`0x0003`) + Information-Class
//! routing — spec `difi-1.2-dataplane` task 4. Mirrors the C++ `test_sample_count`.

use difi::constants::*;
use difi::{Decoder, DifiPacket, IqSample, StreamEncoder};

#[test]
fn sample_count_data_round_trips_with_sample_count_tsf() {
    let mut enc = StreamEncoder::new(0x0000_0042, 16);
    let samples = vec![IqSample { i: 100, q: -200 }, IqSample { i: 7, q: 9 }];
    let bytes = enc
        .encode_data_sample_count(&samples, 1_700_000_000, 4096)
        .unwrap();

    let mut dec = Decoder::new();
    // No Signal Context was sent; a Sample Count Context (0x0003) seeds the depth below,
    // but for data we set it out of band here (format known).
    dec.set_bit_depth(0x0000_0042, 16);
    match dec.decode(&bytes).unwrap() {
        DifiPacket::SignalData(d) => {
            assert_eq!(
                d.header.packet_class,
                packet_class::SAMPLE_COUNT_SIGNAL_DATA
            );
            assert_eq!(
                d.header.information_class,
                information_class::DATA_PLANE_FLOW_CONTROL
            );
            assert_eq!(d.header.tsf, tsf::SAMPLE_COUNT);
            assert_eq!(d.header.fractional_seconds, 4096); // sample count, not ps
            assert_eq!(d.samples, samples);
        }
        other => panic!("expected SignalData, got {other:?}"),
    }
}

#[test]
fn sample_count_context_round_trips_and_seeds_bit_depth() {
    let enc = StreamEncoder::new(0x0000_0043, 12);
    let bytes = enc
        .encode_context_sample_count(2.4e9, 20e6, 30.72e6, -3.0, 1_700_000_000, 0)
        .unwrap();

    let mut dec = Decoder::new();
    match dec.decode(&bytes).unwrap() {
        DifiPacket::SignalContext(c) => {
            assert_eq!(
                c.header.packet_class,
                packet_class::SAMPLE_COUNT_SIGNAL_CONTEXT
            );
            assert_eq!(
                c.header.information_class,
                information_class::DATA_PLANE_FLOW_CONTROL
            );
            assert_eq!(c.header.tsf, tsf::SAMPLE_COUNT);
            assert_eq!(c.data_format.bit_depth, 12);
            assert!((c.sample_rate_hz - 30.72e6).abs() < 1.0);
        }
        other => panic!("expected SignalContext, got {other:?}"),
    }
    // The decoder learned the bit depth from the 0x0003 context, so subsequent data for
    // this stream decodes without an out-of-band hint.
    let mut enc2 = StreamEncoder::new(0x0000_0043, 12);
    // 12-bit needs a word-aligned block: lcm(16,12)/12 = 4 samples.
    let four = vec![
        IqSample { i: 1, q: 2 },
        IqSample { i: 3, q: 4 },
        IqSample { i: 5, q: 6 },
        IqSample { i: 7, q: 8 },
    ];
    let data = enc2
        .encode_data_sample_count(&four, 1_700_000_000, 1)
        .unwrap();
    assert!(matches!(
        dec.decode(&data).unwrap(),
        DifiPacket::SignalData(_)
    ));
}

#[test]
fn sample_count_context_wire_layout_matches_standard_context() {
    // The sample-count context (0x0003) must be byte-identical to the standard context
    // (0x0001) except for the class code (word 3 low 16 bits) and the TSF nibble (word 0).
    let enc = StreamEncoder::new(0x0000_0044, 8);
    let std = enc
        .encode_context(1e9, 1e6, 2e6, 0.0, 1_700_000_000, 0)
        .unwrap();
    let sc = enc
        .encode_context_sample_count(1e9, 1e6, 2e6, 0.0, 1_700_000_000, 0)
        .unwrap();
    assert_eq!(std.len(), sc.len());
    assert_eq!(std.len(), context::WORDS as usize * 4);
    for (off, (a, b)) in std.iter().zip(sc.iter()).enumerate() {
        // Differences allowed only in word 0 (bytes 0-3: TSF/info differ) and word 3
        // bytes 12-15 (information + packet class codes).
        let in_word0 = off < 4;
        let in_word3 = (12..16).contains(&off);
        if a != b {
            assert!(in_word0 || in_word3, "unexpected diff at byte {off}");
        }
    }
}

#[test]
fn standard_flow_1_1_context_still_decodes_unchanged() {
    // Regression: the relaxed class check must not change 1.1 behaviour.
    let enc = StreamEncoder::new(0x0000_0045, 16);
    let bytes = enc
        .encode_context(1e9, 1e6, 2e6, 0.0, 1_700_000_000, 0)
        .unwrap();
    let mut dec = Decoder::new();
    match dec.decode(&bytes).unwrap() {
        DifiPacket::SignalContext(c) => {
            assert_eq!(c.header.packet_class, packet_class::SIGNAL_CONTEXT);
            assert_eq!(c.header.tsf, tsf::REAL_TIME_PS);
        }
        other => panic!("expected SignalContext, got {other:?}"),
    }
}

#[test]
fn unknown_context_class_still_rejected() {
    // A type-0x4 context with a class that is neither 0x0001 nor 0x0003 is fail-closed.
    let enc = StreamEncoder::new(1, 16);
    let mut bytes = enc.encode_context(1e9, 1e6, 2e6, 0.0, 1, 0).unwrap();
    bytes[14] = 0x00;
    bytes[15] = 0x77; // bogus class 0x0077
    assert!(difi::SignalContextPacket::unpack(&bytes).is_err());
}
