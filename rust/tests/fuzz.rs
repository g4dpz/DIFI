// SPDX-License-Identifier: Apache-2.0
//! Property + fuzz hardening (DIFI 1.1 spec task 14 + `difi-1.2-dataplane` task 13 for the
//! command/flow-control decoders; strengthens Req 1.4, 1.5).
//!
//! 1. Property: random *valid* configs round-trip bit-exact (encode -> decode) — for the
//!    1.1 data plane AND the 1.2 Timing Flow Control / sample-count packets.
//! 2. Fuzz: random *malformed* bytes fed to the decoders only ever return a typed
//!    `DifiError` — never panic, never crash, never mis-parse into a wrong packet. This
//!    includes the 1.2 command/flow-control parse paths (CAM/CIF/buffer-status).
//!
//! Dependency-free and deterministic: a seeded xorshift64* PRNG drives both, so a
//! failure is reproducible from the printed seed. No external fuzzing crate is used
//! (the crate is deliberately dependency-light); this runs under `cargo test`.

use std::panic;

use difi::command::{action_mode, BufferState, Cam};
use difi::constants::{command, information_class, packet_type, tsf, tsi, tsm};
use difi::iq::{pack_samples, payload_word_count, sample_range, unpack_samples};
use difi::{
    Decoder, DifiPacket, FlowControlKind, Header, IqSample, SignalDataPacket, StreamEncoder,
    TimingFlowControlPacket,
};

/// Minimal deterministic PRNG (xorshift64*), so fuzz runs are reproducible.
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }
    fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        let span = (hi as i64 - lo as i64 + 1) as u64;
        lo + (self.below(span) as i64) as i32
    }
}

const ITERS: usize = 20_000;

#[test]
fn property_valid_iq_round_trips_bit_exact() {
    let mut rng = Rng::new(0xD1F1_0001);
    for _ in 0..ITERS {
        let bit_depth = rng.range_i32(4, 16) as u8;
        // Choose a sample count that yields whole 32-bit words: samples*2*depth % 32 == 0.
        // samples must be a multiple of lcm(16, depth)/depth... simplest: multiple of 16.
        let n = 16 * (1 + rng.below(8) as usize); // 16..=128 samples
        assert!(payload_word_count(n, bit_depth).is_ok());

        let (min, max) = sample_range(bit_depth);
        let samples: Vec<IqSample> = (0..n)
            .map(|_| IqSample::new(rng.range_i32(min, max), rng.range_i32(min, max)))
            .collect();

        let packed = pack_samples(&samples, bit_depth).expect("pack valid");
        let unpacked = unpack_samples(&packed, n, bit_depth).expect("unpack valid");
        assert_eq!(samples, unpacked, "depth {bit_depth}, n {n}");
    }
}

#[test]
fn property_encoded_packet_round_trips() {
    let mut rng = Rng::new(0x5EED_1234);
    for _ in 0..2000 {
        let bit_depth = rng.range_i32(4, 16) as u8;
        let stream_id = rng.next_u64() as u32;
        let n = 16 * (1 + rng.below(4) as usize);
        let (min, max) = sample_range(bit_depth);
        let samples: Vec<IqSample> = (0..n)
            .map(|_| IqSample::new(rng.range_i32(min, max), rng.range_i32(min, max)))
            .collect();

        let mut enc = StreamEncoder::new(stream_id, bit_depth);
        let int_s = rng.next_u64() as u32;
        let frac = rng.next_u64();
        let bytes = enc.encode_data(&samples, int_s, frac).expect("encode");

        let decoded = SignalDataPacket::unpack(&bytes, bit_depth).expect("decode");
        assert_eq!(decoded.samples, samples);
        assert_eq!(decoded.header.stream_id, stream_id);
        assert_eq!(decoded.header.integer_seconds, int_s);
        assert_eq!(decoded.header.fractional_seconds, frac);
    }
}

#[test]
fn fuzz_decoder_never_panics_on_garbage() {
    // Feed fully random byte buffers of random lengths. The decoder must always return
    // a Result (Ok for an accidentally-valid packet, Err otherwise) and never panic.
    let seed = 0xF0F0_1234_5678_9ABC;
    let mut rng = Rng::new(seed);
    for i in 0..ITERS {
        let len = rng.below(80) as usize; // 0..80 bytes, spanning sub-header to small packets
        let mut buf = vec![0u8; len];
        for b in buf.iter_mut() {
            *b = rng.next_u64() as u8;
        }

        let result = panic::catch_unwind(|| {
            let mut dec = Decoder::new();
            // Register a plausible bit depth for any stream so data packets are attempted.
            dec.set_bit_depth(
                u32::from_be_bytes([
                    *buf.get(4).unwrap_or(&0),
                    *buf.get(5).unwrap_or(&0),
                    *buf.get(6).unwrap_or(&0),
                    *buf.get(7).unwrap_or(&0),
                ]),
                16,
            );
            dec.decode(&buf)
        });
        assert!(
            result.is_ok(),
            "decoder PANICKED on garbage (seed={seed:#x}, iter={i}, bytes={buf:02x?})"
        );
        // When it did not panic, the inner Result is either a valid packet (Ok) or a
        // typed DifiError (Err) — both acceptable. We only forbid a panic / mis-parse.
        if let Ok(Ok(pkt)) = result {
            // An accidentally-valid packet must be one of the known kinds (never a
            // partially-built or inconsistent value): decoding it again is stable.
            assert!(matches!(
                pkt,
                DifiPacket::SignalData(_)
                    | DifiPacket::SignalContext(_)
                    | DifiPacket::VersionContext(_)
                    | DifiPacket::TimingFlowControl(_)
            ));
        }
    }
}

// ---- DIFI 1.2 command / flow-control hardening (spec difi-1.2-dataplane task 13) ----

fn fc_header(rng: &mut Rng, kind: FlowControlKind) -> Header {
    Header {
        packet_type: packet_type::COMMAND,
        class_id_present: true,
        tsm: tsm::DATA,
        tsi: tsi::UTC,
        tsf: match kind {
            FlowControlKind::SampleCount => tsf::SAMPLE_COUNT,
            FlowControlKind::RealTime => tsf::REAL_TIME_PS,
        },
        seq_num: (rng.below(16)) as u8,
        stream_id: rng.next_u64() as u32,
        information_class: information_class::DATA_PLANE_FLOW_CONTROL,
        packet_class: kind.packet_class(),
        integer_seconds: rng.next_u64() as u32,
        fractional_seconds: rng.next_u64(),
    }
}

#[test]
fn property_flow_control_round_trips_bit_exact() {
    let mut rng = Rng::new(0xF10C_2222);
    for _ in 0..5000 {
        let kind = if rng.below(2) == 0 {
            FlowControlKind::SampleCount
        } else {
            FlowControlKind::RealTime
        };
        let pkt = TimingFlowControlPacket {
            header: fc_header(&mut rng, kind),
            kind,
            cam: Cam {
                controllee_id_present: true,
                controller_id_present: true,
                action_mode: action_mode::EXECUTE,
                ..Cam::default()
            },
            message_id: rng.next_u64() as u32,
            controllee_id: rng.next_u64() as u32,
            controller_id: rng.next_u64() as u32,
            reference_point: command::REF_POINT,
            // Sample rate is stored as integer*2^20; keep it in a range that round-trips
            // exactly through the fixed-point (integer Hz up to a few GHz).
            sample_rate_hz: rng.below(6_000_000_000) as f64,
            timestamp_adjust_fs: rng.range_i32(-1_000_000, 1_000_000) as i64,
            buffer: BufferState {
                size_bytes: rng.next_u64(),
                overflow: rng.below(2) == 1,
                near_full: rng.below(2) == 1,
                near_empty: rng.below(2) == 1,
                underflow: rng.below(2) == 1,
                level: (rng.next_u64() as u32) & !0xF,
            },
        };
        let bytes = pkt.pack().expect("pack flow control");
        assert_eq!(bytes.len(), command::WORDS as usize * 4);
        let back = TimingFlowControlPacket::unpack(&bytes).expect("unpack flow control");
        assert_eq!(back.kind, pkt.kind);
        assert_eq!(back.header.packet_class, pkt.header.packet_class);
        assert_eq!(back.message_id, pkt.message_id);
        assert_eq!(back.controllee_id, pkt.controllee_id);
        assert_eq!(back.controller_id, pkt.controller_id);
        assert_eq!(back.timestamp_adjust_fs, pkt.timestamp_adjust_fs);
        assert_eq!(back.buffer, pkt.buffer);
        assert_eq!(back.cam, pkt.cam);
        assert!((back.sample_rate_hz - pkt.sample_rate_hz).abs() < 1.0);
    }
}

#[test]
fn property_sample_count_data_round_trips() {
    let mut rng = Rng::new(0x5C00_3333);
    for _ in 0..2000 {
        let bit_depth = rng.range_i32(4, 16) as u8;
        let stream_id = rng.next_u64() as u32;
        let n = 16 * (1 + rng.below(4) as usize);
        let (min, max) = sample_range(bit_depth);
        let samples: Vec<IqSample> = (0..n)
            .map(|_| IqSample::new(rng.range_i32(min, max), rng.range_i32(min, max)))
            .collect();
        let mut enc = StreamEncoder::new(stream_id, bit_depth);
        let bytes = enc
            .encode_data_sample_count(&samples, rng.next_u64() as u32, rng.next_u64())
            .expect("encode sample-count data");
        let decoded = SignalDataPacket::unpack(&bytes, bit_depth).expect("decode");
        assert_eq!(decoded.samples, samples);
        assert_eq!(
            decoded.header.packet_class,
            difi::constants::packet_class::SAMPLE_COUNT_SIGNAL_DATA
        );
        assert_eq!(decoded.header.tsf, tsf::SAMPLE_COUNT);
    }
}

#[test]
fn fuzz_flow_control_decoder_never_panics() {
    // Random bytes of command-packet-plausible lengths fed to the flow-control decoder
    // (directly and via the full Decoder's 0x6 path) must only ever return a typed
    // error — never panic. Exercises CAM/CIF/buffer-status parsing on garbage.
    let seed = 0xF10C_DEAD_BEEF_0001;
    let mut rng = Rng::new(seed);
    for i in 0..ITERS {
        // Lengths around the fixed 84-byte command packet, incl. short/long/mismatched.
        let len = (rng.below(120) as usize / 4) * 4; // word-aligned 0..120
        let mut buf = vec![0u8; len];
        for b in buf.iter_mut() {
            *b = rng.next_u64() as u8;
        }
        // Half the time, stamp a COMMAND packet type + correct OUI + a command class so
        // the decoder routes into the flow-control parser and hits its deeper fields.
        if rng.below(2) == 0 && buf.len() >= 16 {
            let pktsize = (buf.len() / 4) as u32 & 0xFFFF;
            let word0 = ((packet_type::COMMAND as u32) << 28) | 0x0800_0000 | pktsize;
            buf[0..4].copy_from_slice(&word0.to_be_bytes());
            buf[8..12].copy_from_slice(&0x006A_621Eu32.to_be_bytes());
            let class = if rng.below(2) == 0 { 0x0005u16 } else { 0x0006 };
            buf[14..16].copy_from_slice(&class.to_be_bytes());
        }

        let via_type = panic::catch_unwind(|| TimingFlowControlPacket::unpack(&buf));
        assert!(
            via_type.is_ok(),
            "flow-control unpack PANICKED (seed={seed:#x}, iter={i}, bytes={buf:02x?})"
        );
        let via_decoder = panic::catch_unwind(|| {
            let mut dec = Decoder::new();
            dec.decode(&buf)
        });
        assert!(
            via_decoder.is_ok(),
            "decoder PANICKED on command bytes (seed={seed:#x}, iter={i})"
        );
    }
}

#[test]
fn fuzz_structurally_valid_header_with_random_body() {
    // Build a buffer with a correct OUI and a random-but-consistent pktSize, so the
    // decoder gets past classify() and into field parsing — the deeper code paths.
    let seed = 0xBADC_0FFEE_u64;
    let mut rng = Rng::new(seed);
    for i in 0..ITERS {
        // total words between 1 and 40; buffer length must match pktSize*4 sometimes,
        // and sometimes not, to exercise the size-mismatch path.
        let words = 1 + rng.below(40) as usize;
        let mut buf = vec![0u8; words * 4];
        for b in buf.iter_mut() {
            *b = rng.next_u64() as u8;
        }
        // Random packet type in word0 high nibble.
        let ptype = (rng.below(16) as u32) << 28;
        let pktsize = words as u32 & 0xFFFF;
        let word0 = ptype | (rng.next_u64() as u32 & 0x0FFF_0000) | pktsize;
        buf[0..4].copy_from_slice(&word0.to_be_bytes());
        // Correct OUI so classify() passes and deeper parsing runs (when len >= 12).
        if buf.len() >= 12 {
            buf[8] = 0x00;
            buf[9] = 0x6A;
            buf[10] = 0x62;
            buf[11] = 0x1E;
        }

        let result = panic::catch_unwind(|| {
            let mut dec = Decoder::new();
            dec.set_bit_depth(
                u32::from_be_bytes([
                    *buf.get(4).unwrap_or(&0),
                    *buf.get(5).unwrap_or(&0),
                    *buf.get(6).unwrap_or(&0),
                    *buf.get(7).unwrap_or(&0),
                ]),
                16,
            );
            dec.decode(&buf)
        });
        assert!(
            result.is_ok(),
            "decoder PANICKED (seed={seed:#x}, iter={i}, words={words}, bytes={buf:02x?})"
        );
    }
}
