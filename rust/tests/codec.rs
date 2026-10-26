//! Unit + integration tests for the DIFI 1.1 Rust codec (spec tasks 2–7).

use difi::constants::*;
use difi::iq::{pack_samples, payload_word_count, sample_range, unpack_samples};
use difi::{
    DataFormat, Decoder, DifiError, DifiPacket, Header, IqSample, SignalContextPacket,
    SignalDataPacket, StreamEncoder, VersionContextPacket,
};

fn header(packet_type: u8, information_class: u16, packet_class: u16, seq: u8) -> Header {
    let tsm = if packet_type == packet_type::SIGNAL_DATA {
        tsm::DATA
    } else {
        tsm::CONTEXT
    };
    Header {
        packet_type,
        class_id_present: true,
        tsm,
        tsi: tsi::UTC,
        tsf: tsf::REAL_TIME_PS,
        seq_num: seq,
        stream_id: 0xDEAD_BEEF,
        information_class,
        packet_class,
        integer_seconds: 1_700_000_000,
        fractional_seconds: 42_000_000_000,
    }
}

#[test]
fn iq_bit_exact_round_trip_all_depths() {
    // Task 3: bit-exact at representative depths including sub-byte packing.
    for &depth in &[4u8, 8, 12, 16] {
        let (min, max) = sample_range(depth);
        // Choose a sample count that yields whole words at this depth.
        // 32 complex samples * 2 * depth is always a multiple of 32 for depth in {4,8,12,16}? 
        // 32*2*12 = 768 (÷32 = 24 ok). Use 16 samples: 16*2*depth = 32*depth → always ÷32.
        let n = 16usize;
        let samples: Vec<IqSample> = (0..n)
            .map(|k| {
                let i = min + (k as i32 % (max - min + 1));
                let q = max - (k as i32 % (max - min + 1));
                IqSample::new(i, q)
            })
            .collect();
        let packed = pack_samples(&samples, depth).unwrap();
        assert_eq!(packed.len() % 4, 0, "depth {depth} not word aligned");
        let unpacked = unpack_samples(&packed, n, depth).unwrap();
        assert_eq!(samples, unpacked, "round trip failed at depth {depth}");
    }
}

#[test]
fn iq_extremes_round_trip() {
    for &depth in &[4u8, 5, 7, 8, 11, 12, 15, 16] {
        let (min, max) = sample_range(depth);
        // 16 samples keeps alignment for any depth (16*2*d = 32*d).
        let mut samples = vec![IqSample::new(min, max); 8];
        samples.extend(vec![IqSample::new(max, min); 8]);
        let packed = pack_samples(&samples, depth).unwrap();
        let unpacked = unpack_samples(&packed, samples.len(), depth).unwrap();
        assert_eq!(samples, unpacked, "extremes failed at depth {depth}");
    }
}

#[test]
fn iq_rejects_bad_depth_and_misalignment() {
    assert!(matches!(
        pack_samples(&[IqSample::new(0, 0)], 3),
        Err(DifiError::BadBitDepth { .. })
    ));
    assert!(matches!(
        pack_samples(&[IqSample::new(0, 0)], 17),
        Err(DifiError::BadBitDepth { .. })
    ));
    // 1 sample * 2 * 12 = 24 bits → not word aligned.
    assert!(matches!(
        payload_word_count(1, 12),
        Err(DifiError::NotWordAligned { .. })
    ));
}

#[test]
fn iq_rejects_out_of_range_sample() {
    let (_, max) = sample_range(8);
    assert!(matches!(
        pack_samples(&[IqSample::new(max + 1, 0); 16], 8),
        Err(DifiError::SampleOutOfRange { .. })
    ));
}

#[test]
fn signal_data_round_trip() {
    // Task 2 + 6: pack→unpack bit-exact.
    let samples: Vec<IqSample> = (0..16).map(|k| IqSample::new(k - 8, 8 - k)).collect();
    let pkt = SignalDataPacket {
        header: header(
            packet_type::SIGNAL_DATA,
            information_class::STANDARD_FLOW,
            packet_class::SIGNAL_DATA,
            5,
        ),
        bit_depth: 16,
        samples: samples.clone(),
    };
    let bytes = pkt.pack().unwrap();
    assert_eq!(bytes.len() % 4, 0);
    let decoded = SignalDataPacket::unpack(&bytes, 16).unwrap();
    assert_eq!(decoded.header, pkt.header);
    assert_eq!(decoded.samples, samples);
}

fn sample_context_header() -> Header {
    header(
        packet_type::CONTEXT,
        information_class::STANDARD_FLOW,
        packet_class::SIGNAL_CONTEXT,
        0,
    )
}

fn sample_context(rf: f64, bw: f64, fs: f64, gain: f64, bit_depth: u8) -> SignalContextPacket {
    SignalContextPacket {
        header: sample_context_header(),
        context_changed: true,
        rf_reference_hz: rf,
        if_reference_hz: 0.0,
        bandwidth_hz: bw,
        sample_rate_hz: fs,
        gain_db: gain,
        data_format: DataFormat { bit_depth },
    }
}

#[test]
fn signal_context_round_trip() {
    let pkt = sample_context(2.401e9, 1.0e6, 1.25e6, -3.5, 16);
    let bytes = pkt.pack().unwrap();
    assert_eq!(bytes.len(), 27 * 4, "context must be 27 words");
    let decoded = SignalContextPacket::unpack(&bytes).unwrap();
    // Fixed-point scaling means values round-trip within a small tolerance.
    assert!((decoded.rf_reference_hz - pkt.rf_reference_hz).abs() < 1.0);
    assert!((decoded.bandwidth_hz - pkt.bandwidth_hz).abs() < 1.0);
    assert!((decoded.sample_rate_hz - pkt.sample_rate_hz).abs() < 1.0);
    assert!((decoded.gain_db - pkt.gain_db).abs() < 0.01);
    assert_eq!(decoded.data_format.bit_depth, 16);
    assert_eq!(decoded.header, pkt.header);
}

#[test]
fn version_context_round_trip() {
    let pkt = VersionContextPacket {
        header: header(
            version_packet_type::VERSION_CONTEXT,
            information_class::VERSION_FLOW,
            packet_class::VERSION_CONTEXT,
            0,
        ),
        context_changed: true,
        year: 2021,
        day: 100,
    };
    let bytes = pkt.pack().unwrap();
    assert_eq!(bytes.len(), 11 * 4, "version must be 11 words");
    let decoded = VersionContextPacket::unpack(&bytes).unwrap();
    assert_eq!(decoded, pkt);
}

#[test]
fn decoder_fail_closed() {
    let mut dec = Decoder::new();

    // Bad OUI.
    let mut good = sample_context(1.0, 1.0, 1.0, 0.0, 16).pack().unwrap();
    // Corrupt OUI (bytes 8..12, low 24 bits).
    good[9] = 0xFF;
    assert!(matches!(dec.decode(&good), Err(DifiError::BadOui { .. })));

    // Truncated buffer.
    assert!(matches!(
        dec.decode(&[0u8; 10]),
        Err(DifiError::Truncated { .. })
    ));

    // Size mismatch: valid header but extra trailing bytes.
    let mut ctx = sample_context(1.0, 1.0, 1.0, 0.0, 16).pack().unwrap();
    ctx.extend_from_slice(&[0, 0, 0, 0]);
    assert!(matches!(
        dec.decode(&ctx),
        Err(DifiError::SizeMismatch { .. })
    ));
}

#[test]
fn stream_encoder_sequences_mod16() {
    // Task 6 / Req 2.3: mod-16 sequence across data packets.
    let mut enc = StreamEncoder::new(0xABCD_0001, 16);
    let samples = vec![IqSample::new(1, -1); 16];
    let mut dec = Decoder::new();
    dec.set_bit_depth(0xABCD_0001, 16);
    for n in 0..20u32 {
        let bytes = enc.encode_data(&samples, 0, 0).unwrap();
        match dec.decode(&bytes).unwrap() {
            DifiPacket::SignalData(d) => {
                assert_eq!(d.header.seq_num, (n % 16) as u8);
                assert_eq!(d.samples, samples);
            }
            _ => panic!("expected signal data"),
        }
    }
}

#[test]
fn stateful_decode_learns_bit_depth_from_context() {
    // A receiver with no out-of-band hint learns the depth from the context packet.
    let mut enc = StreamEncoder::new(0x55, 12);
    let ctx = enc.encode_context(1e9, 1e6, 1e6, 0.0, 0, 0).unwrap();
    let samples = vec![IqSample::new(3, -3); 16];
    let data = enc.encode_data(&samples, 0, 0).unwrap();

    let mut dec = Decoder::new();
    // Data before context must fail (no format known).
    assert!(dec.decode(&data).is_err());
    // Context, then data, succeeds.
    assert!(matches!(
        dec.decode(&ctx).unwrap(),
        DifiPacket::SignalContext(_)
    ));
    match dec.decode(&data).unwrap() {
        DifiPacket::SignalData(d) => assert_eq!(d.samples, samples),
        _ => panic!("expected data"),
    }
}
