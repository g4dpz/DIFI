// SPDX-License-Identifier: Apache-2.0
//! `difi-tool` — minimal CLI for cross-implementation interop.
//!
//! Subcommands:
//!   gen-golden <dir>   Write the canonical golden packets to <dir> (shared fixtures).
//!   dump <file>        Decode a raw packet file and print its fields as text.
//!
//! The golden packets written here are the shared conformance oracle; the C++
//! implementation must produce byte-identical output and must decode these.

use std::env;
use std::fs;
use std::process::ExitCode;
use std::time::Duration;

use difi::constants::packet_class;
use difi::{
    generate_signal, measure_implementation_loss, Decoder, DifiPacket, IqSample,
    ScalingConvention, SignalParams, StreamEncoder, UdpReceiver, UdpSender,
};

const STREAM_ID: u32 = 0x1234_5678;
const INT_SECONDS: u32 = 1_700_000_000;
const FRAC_PS: u64 = 123_456_789_000;

/// End-to-end export config shared by `export` and `import` (both sides regenerate the
/// same reference signal, so the receiver can compare against the known original).
const EXPORT_BIT_DEPTH: u8 = 16;
const EXPORT_FULL_SCALE: f64 = 1.0;
const EXPORT_SAMPLES_PER_PACKET: usize = 256;

/// A fixed float32 I/Q vector exercising zero, mid, full-scale, over-range and
/// negative values — used to prove both implementations map floats to identical codes.
fn map_check_vector() -> Vec<(f32, f32)> {
    vec![
        (0.0, 0.0),
        (0.5, -0.5),
        (1.0, -1.0),
        (1.5, -1.5),   // over-range: must saturate
        (-1.5, 1.5),   // over-range negative
        (0.123_456, -0.987_654),
        (0.000_030_5, -0.000_030_5), // ~1 LSB at 16-bit
        (0.999_99, -0.999_99),
    ]
}

/// Map the fixed vector at a bit depth and print `code I Q` lines, so the two
/// implementations' integer output can be diffed for exact equality.
fn map_check(bit_depth: u8) -> Result<(), String> {
    let conv = difi::ScalingConvention::new(bit_depth, 1.0).map_err(|e| e.to_string())?;
    let v = map_check_vector();
    let codes = difi::float_to_difi(&v, &conv);
    println!("bit_depth={bit_depth} fs_code={}", conv.fs_code());
    for s in &codes {
        println!("{} {}", s.i, s.q);
    }
    let report = difi::measure_implementation_loss(&v, &conv);
    // Print loss with fixed precision so both tools agree textually.
    println!(
        "loss max_abs={:.9} rms={:.9}",
        report.max_abs_error, report.rms_error
    );
    Ok(())
}

/// Deterministic I/Q stream used by the send/recv interop test: packet `p` holds
/// 8 samples whose values derive from the packet index, so the receiver can verify
/// exact recovery without a side channel.
fn stream_samples(packet_index: u32) -> Vec<IqSample> {
    (0..8)
        .map(|n| {
            let base = (packet_index * 8 + n) as i32;
            IqSample::new(base - 16, 16 - base)
        })
        .collect()
}

fn golden_samples() -> Vec<IqSample> {
    // A small, deterministic ramp exercising positive, negative and extreme values.
    (0..8)
        .map(|n| IqSample::new((n as i32) - 4, 100 - (n as i32) * 25))
        .collect()
}

fn write_golden(dir: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let bit_depth = 16u8;
    let mut enc = StreamEncoder::new(STREAM_ID, bit_depth);

    let ctx = enc
        .encode_context(2.401e9, 1.0e6, 1.25e6, -3.5, INT_SECONDS, FRAC_PS)
        .expect("context");
    fs::write(format!("{dir}/signal_context_16bit.difi.bin"), &ctx)?;

    let ver = enc
        .encode_version(2021, 100, INT_SECONDS, FRAC_PS)
        .expect("version");
    fs::write(format!("{dir}/version_context.difi.bin"), &ver)?;

    let data = enc
        .encode_data(&golden_samples(), INT_SECONDS, FRAC_PS)
        .expect("data");
    fs::write(format!("{dir}/signal_data_16bit.difi.bin"), &data)?;

    println!("wrote 3 golden packets to {dir}");
    Ok(())
}

fn dump(file: &str) -> Result<(), String> {
    let buf = fs::read(file).map_err(|e| e.to_string())?;
    let mut dec = Decoder::new();
    // A standalone Signal Data packet needs an out-of-band bit depth (normally learned
    // from a preceding Signal Context). Golden/cross-decode data is 16-bit; register it
    // for whatever stream this packet carries.
    dec.set_bit_depth(STREAM_ID, 16);
    if let Some(sid) = difi::peek_stream_id(&buf) {
        dec.set_bit_depth(sid, 16);
    }
    match dec.decode(&buf).map_err(|e| e.to_string())? {
        DifiPacket::SignalContext(c) => {
            println!("SignalContext stream=0x{:08X} class=0x{:04X}", c.header.stream_id, packet_class::SIGNAL_CONTEXT);
            println!("  rf={} Hz bw={} Hz fs={} Hz gain={} dB bit_depth={}",
                c.rf_reference_hz, c.bandwidth_hz, c.sample_rate_hz, c.gain_db, c.data_format.bit_depth);
        }
        DifiPacket::VersionContext(v) => {
            println!("VersionContext stream=0x{:08X}", v.header.stream_id);
            println!("  year={} day={} changed={}", v.year, v.day, v.context_changed);
        }
        DifiPacket::SignalData(d) => {
            println!("SignalData stream=0x{:08X} seq={} bit_depth={} samples={}",
                d.header.stream_id, d.header.seq_num, d.bit_depth, d.samples.len());
            for (n, s) in d.samples.iter().enumerate() {
                println!("  [{n}] I={} Q={}", s.i, s.q);
            }
        }
        DifiPacket::TimingFlowControl(f) => {
            println!("TimingFlowControl stream=0x{:08X} class=0x{:04X} kind={:?}",
                f.header.stream_id, f.header.packet_class, f.kind);
            println!("  controllee=0x{:08X} controller=0x{:08X} msg_id={}",
                f.controllee_id, f.controller_id, f.message_id);
            println!("  fs={} Hz ts_adj={} fs buffer={} B OF={} NF={} NE={} UF={}",
                f.sample_rate_hz, f.timestamp_adjust_fs, f.buffer.size_bytes,
                f.buffer.overflow, f.buffer.near_full, f.buffer.near_empty, f.buffer.underflow);
        }
    }
    Ok(())
}

/// Send a short DIFI stream over UDP: one Signal Context packet, then `count` Signal
/// Data packets with running mod-16 sequence numbers.
fn send(dest: &str, count: u32) -> Result<(), String> {
    let bit_depth = 16u8;
    let mut enc = StreamEncoder::new(STREAM_ID, bit_depth);
    let sender = UdpSender::new(dest).map_err(|e| e.to_string())?;

    let ctx = enc
        .encode_context(2.401e9, 1.0e6, 1.25e6, -3.5, INT_SECONDS, FRAC_PS)
        .map_err(|e| e.to_string())?;
    sender.send(&ctx).map_err(|e| e.to_string())?;

    for p in 0..count {
        let data = enc
            .encode_data(&stream_samples(p), INT_SECONDS, FRAC_PS + p as u64)
            .map_err(|e| e.to_string())?;
        sender.send(&data).map_err(|e| e.to_string())?;
        // Tiny gap so a localhost receiver's socket buffer does not overflow on bursts.
        std::thread::sleep(Duration::from_millis(2));
    }
    println!("sent 1 context + {count} data packets to {dest} (stream 0x{STREAM_ID:08X})");
    Ok(())
}

/// Send a short DIFI stream over the NON-STANDARD TCP dev transport (Req 5.2):
/// one Signal Context packet, then `count` Signal Data packets, length-framed.
fn tcp_send(dest: &str, count: u32) -> Result<(), String> {
    let mut enc = StreamEncoder::new(STREAM_ID, 16);
    let mut sender = difi::TcpSender::connect(dest).map_err(|e| e.to_string())?;
    let ctx = enc
        .encode_context(2.401e9, 1.0e6, 1.25e6, -3.5, INT_SECONDS, FRAC_PS)
        .map_err(|e| e.to_string())?;
    sender.send(&ctx).map_err(|e| e.to_string())?;
    for p in 0..count {
        let data = enc
            .encode_data(&stream_samples(p), INT_SECONDS, FRAC_PS + p as u64)
            .map_err(|e| e.to_string())?;
        sender.send(&data).map_err(|e| e.to_string())?;
    }
    println!("tcp-sent 1 context + {count} data packets to {dest} (stream 0x{STREAM_ID:08X})");
    Ok(())
}

/// Receive a DIFI stream over the NON-STANDARD TCP dev transport, verifying each data
/// packet's samples and sequence number. Prints `RECV OK <n>` on success.
fn tcp_recv(bind_addr: &str, count: u32) -> Result<(), String> {
    let mut rx = difi::TcpReceiver::bind(bind_addr).map_err(|e| e.to_string())?;
    rx.set_stream_filter(Some(STREAM_ID));
    let mut dec = Decoder::new();

    let mut data_seen = 0u32;
    let mut ctx_seen = 0u32;
    while data_seen < count {
        let buf = match rx.recv().map_err(|e| format!("tcp recv: {e}"))? {
            Some(b) => b,
            None => return Err(format!("stream closed after {data_seen}/{count} data packets")),
        };
        match dec.decode(&buf).map_err(|e| e.to_string())? {
            DifiPacket::SignalContext(c) => {
                ctx_seen += 1;
                if c.data_format.bit_depth != 16 {
                    return Err(format!("unexpected bit depth {}", c.data_format.bit_depth));
                }
            }
            DifiPacket::SignalData(d) => {
                let expected = stream_samples(data_seen);
                if d.samples != expected {
                    return Err(format!("sample mismatch at data packet {data_seen}"));
                }
                if d.header.seq_num != (data_seen % 16) as u8 {
                    return Err(format!("seq mismatch at packet {data_seen}: got {}", d.header.seq_num));
                }
                data_seen += 1;
            }
            DifiPacket::VersionContext(_) | DifiPacket::TimingFlowControl(_) => {}
        }
    }
    if ctx_seen == 0 {
        return Err("no Signal Context packet received before data".into());
    }
    println!("RECV OK {data_seen}");
    Ok(())
}

/// Receive a DIFI stream on `bind_addr`, filter by the known Stream ID, decode, and
/// verify each data packet's samples and sequence number against the expected stream.
/// Prints `RECV OK <n>` on full success so the interop harness can assert on it.
fn recv(bind_addr: &str, count: u32) -> Result<(), String> {
    let mut rx = UdpReceiver::bind(bind_addr).map_err(|e| e.to_string())?;
    rx.set_stream_filter(Some(STREAM_ID));
    rx.set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    let mut dec = Decoder::new();

    let mut data_seen = 0u32;
    let mut ctx_seen = 0u32;
    // Expect 1 context + `count` data packets.
    while data_seen < count {
        let (buf, _from) = rx.recv().map_err(|e| format!("recv: {e}"))?;
        match dec.decode(&buf).map_err(|e| e.to_string())? {
            DifiPacket::SignalContext(c) => {
                ctx_seen += 1;
                if c.data_format.bit_depth != 16 {
                    return Err(format!("unexpected bit depth {}", c.data_format.bit_depth));
                }
            }
            DifiPacket::SignalData(d) => {
                let expected = stream_samples(data_seen);
                if d.samples != expected {
                    return Err(format!("sample mismatch at data packet {data_seen}"));
                }
                if d.header.seq_num != (data_seen % 16) as u8 {
                    return Err(format!(
                        "seq mismatch at packet {data_seen}: got {}",
                        d.header.seq_num
                    ));
                }
                data_seen += 1;
            }
            DifiPacket::VersionContext(_) | DifiPacket::TimingFlowControl(_) => {}
        }
    }
    if ctx_seen == 0 {
        return Err("no Signal Context packet received before data".into());
    }
    println!("RECV OK {data_seen}");
    Ok(())
}

const REFERENCE_SIGNAL_NAME: &str = "afs-style-tone-4096";

/// Build the manifest for the default reference signal from freshly generated bytes.
fn reference_manifest(raw_bytes: &[u8]) -> difi::Manifest {
    let params = SignalParams::default();
    let difi_ctx = difi::DifiContext {
        bit_depth: EXPORT_BIT_DEPTH,
        full_scale: EXPORT_FULL_SCALE,
        rf_reference_hz: 2.401e9,
        bandwidth_hz: params.sample_rate_hz,
        sample_rate_hz: params.sample_rate_hz,
        gain_db: -3.5,
    };
    difi::build_manifest(
        REFERENCE_SIGNAL_NAME,
        raw_bytes,
        params,
        difi_ctx,
        &format!("difi-tool generate <work-dir> (writes {REFERENCE_SIGNAL_NAME}.cf32)"),
    )
}

/// Generate the reference signal (Req 6.1, 6.3): write the large raw cf32 to `work_dir`
/// (gitignored — the raw I/Q never enters Git) and the small manifest to the tracked
/// `signals/` directory. The manifest + generator reproduce the raw bytes on demand.
fn generate_to_dir(work_dir: &str) -> Result<(), String> {
    let params = SignalParams::default();
    let iq = generate_signal(&params);
    let raw = difi::manifest::encode_cf32(&iq);
    let manifest = reference_manifest(&raw);

    fs::create_dir_all(work_dir).map_err(|e| e.to_string())?;
    let raw_path = format!("{work_dir}/{REFERENCE_SIGNAL_NAME}.cf32");
    fs::write(&raw_path, &raw).map_err(|e| e.to_string())?;

    // Manifest goes to the tracked signals/ dir (committable), not the gitignored work dir.
    fs::create_dir_all("signals").map_err(|e| e.to_string())?;
    let manifest_path = format!("signals/{REFERENCE_SIGNAL_NAME}.manifest.json");
    fs::write(&manifest_path, manifest.to_json()).map_err(|e| e.to_string())?;

    println!("generated {} samples", iq.len());
    println!("  raw      {raw_path} ({} bytes, gitignored)", raw.len());
    println!("  manifest {manifest_path} (committed)");
    println!("  sha256   {}", manifest.sha256);
    Ok(())
}

/// Verify a committed manifest (Req 6.2 test): regenerate the signal from the manifest's
/// parameters, recompute the SHA-256, and confirm it matches the recorded value.
fn verify_manifest(manifest_path: &str) -> Result<(), String> {
    let text = fs::read_to_string(manifest_path).map_err(|e| e.to_string())?;
    let recorded_sha = json_string_field(&text, "sha256")
        .ok_or("manifest missing sha256 field")?;
    let recorded_size: usize = json_number_field(&text, "byte_size")
        .ok_or("manifest missing byte_size field")?;

    // Regenerate from the committed default parameters (the generator is deterministic).
    let params = SignalParams::default();
    let iq = generate_signal(&params);
    let raw = difi::manifest::encode_cf32(&iq);
    let actual_sha = difi::sha256::hex_digest(&raw);

    println!("verifying {manifest_path}");
    println!("  recorded size={recorded_size} sha256={recorded_sha}");
    println!("  regen    size={} sha256={actual_sha}", raw.len());
    if raw.len() == recorded_size && actual_sha == recorded_sha {
        println!("MANIFEST OK — regenerated signal matches recorded SHA-256");
        Ok(())
    } else {
        Err("regenerated signal does not match the manifest".into())
    }
}

/// Minimal JSON field extractors (the manifest is our own stable format).
fn json_string_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let colon = text[start..].find(':')? + start + 1;
    let q1 = text[colon..].find('"')? + colon + 1;
    let q2 = text[q1..].find('"')? + q1;
    Some(text[q1..q2].to_string())
}
fn json_number_field(text: &str, key: &str) -> Option<usize> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let colon = text[start..].find(':')? + start + 1;
    let rest = text[colon..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Print the DIFI integer codes of the generated reference signal, so the two
/// implementations' generated+mapped output can be diffed for exact equality.
fn gen_check() -> Result<(), String> {
    let params = SignalParams::default();
    let float_iq = generate_signal(&params);
    let conv = ScalingConvention::new(EXPORT_BIT_DEPTH, EXPORT_FULL_SCALE)
        .map_err(|e| e.to_string())?;
    let codes = difi::float_to_difi(&float_iq, &conv);
    println!("samples={} bit_depth={EXPORT_BIT_DEPTH}", codes.len());
    for s in &codes {
        println!("{} {}", s.i, s.q);
    }
    Ok(())
}

/// Export a reference float32 signal over DIFI (Req 7.1): generate -> map to DIFI
/// integer -> encode Signal Context + chunked Signal Data -> UDP.
fn export(dest: &str) -> Result<(), String> {
    let params = SignalParams::default();
    let float_iq = generate_signal(&params);
    let conv = ScalingConvention::new(EXPORT_BIT_DEPTH, EXPORT_FULL_SCALE)
        .map_err(|e| e.to_string())?;
    let codes: Vec<IqSample> = difi::float_to_difi(&float_iq, &conv);

    let mut enc = StreamEncoder::new(STREAM_ID, EXPORT_BIT_DEPTH);
    let sender = UdpSender::new(dest).map_err(|e| e.to_string())?;

    // Context packet describing the stream, then chunked data packets.
    let ctx = enc
        .encode_context(
            2.401e9,
            params.sample_rate_hz,
            params.sample_rate_hz,
            -3.5,
            INT_SECONDS,
            FRAC_PS,
        )
        .map_err(|e| e.to_string())?;
    sender.send(&ctx).map_err(|e| e.to_string())?;

    for chunk in codes.chunks(EXPORT_SAMPLES_PER_PACKET) {
        let data = enc
            .encode_data(chunk, INT_SECONDS, FRAC_PS)
            .map_err(|e| e.to_string())?;
        sender.send(&data).map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(2));
    }
    let n_packets = codes.len().div_ceil(EXPORT_SAMPLES_PER_PACKET);
    println!(
        "exported {} samples in {} data packets (+1 context) to {dest}",
        codes.len(),
        n_packets
    );
    Ok(())
}

/// Import a DIFI signal (Req 7.1, 7.3): receive -> decode -> inverse-map to float ->
/// compare against the regenerated original -> report measured loss and PASS/FAIL.
///
/// `interop` selects the pass criterion:
/// - false (default): same-implementation round trip — end-to-end loss must equal the
///   mapper-only quantisation loss (transport/codec add nothing).
/// - true: cross-stack interop — recovery need only be within one quantisation step
///   (1 LSB). A peer whose float math differs by a ULP (e.g. a different libm) can map
///   an isolated sample to the adjacent code; that is still correct interop.
fn import(bind_addr: &str) -> Result<(), String> {
    import_impl(bind_addr, false)
}

fn import_interop(bind_addr: &str) -> Result<(), String> {
    import_impl(bind_addr, true)
}

fn import_impl(bind_addr: &str, interop: bool) -> Result<(), String> {
    let params = SignalParams::default();
    let original = generate_signal(&params);
    let total = original.len();
    let conv = ScalingConvention::new(EXPORT_BIT_DEPTH, EXPORT_FULL_SCALE)
        .map_err(|e| e.to_string())?;

    // Loss budget: the mapper's own quantisation bound (<=1 LSB per component). The
    // end-to-end path must not exceed the mapper-only loss — if it does, something in
    // encode/transport/decode corrupted samples.
    let mapper_report = measure_implementation_loss(&original, &conv);

    let mut rx = UdpReceiver::bind(bind_addr).map_err(|e| e.to_string())?;
    rx.set_stream_filter(Some(STREAM_ID));
    rx.set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(|e| e.to_string())?;
    let mut dec = Decoder::new();
    // In interop mode the sample format is known out of band (16-bit, per the export
    // convention). Pre-register it so a peer that sends Signal Context on its own
    // cadence — or not before the first data packet, as gr-difi does — still decodes.
    if interop {
        dec.set_bit_depth(STREAM_ID, EXPORT_BIT_DEPTH);
    }

    let mut recovered_codes: Vec<IqSample> = Vec::with_capacity(total);
    let mut ctx_seen = false;
    while recovered_codes.len() < total {
        let (buf, _from) = rx.recv().map_err(|e| format!("recv: {e}"))?;
        match dec.decode(&buf).map_err(|e| e.to_string())? {
            DifiPacket::SignalContext(_) => ctx_seen = true,
            DifiPacket::SignalData(d) => recovered_codes.extend(d.samples),
            DifiPacket::VersionContext(_) | DifiPacket::TimingFlowControl(_) => {}
        }
    }
    if !ctx_seen && !interop {
        return Err("no Signal Context packet received before data".into());
    }

    // Inverse-map to float and measure recovered-vs-original error.
    let recovered: Vec<(f32, f32)> = difi::difi_to_float(&recovered_codes, &conv);
    let mut sum_sq_err = 0.0f64;
    let mut max_abs = 0.0f64;
    for (&(oi, oq), &(ri, rq)) in original.iter().zip(recovered.iter()) {
        for (o, r) in [(oi, ri), (oq, rq)] {
            let e = (r as f64) - (o as f64);
            sum_sq_err += e * e;
            max_abs = max_abs.max(e.abs());
        }
    }
    let rms = (sum_sq_err / (total * 2) as f64).sqrt();

    // PASS criteria depend on the mode (see import_impl docs).
    let one_lsb = EXPORT_FULL_SCALE / conv.fs_code() as f64;
    let bound = if interop {
        one_lsb // cross-stack: within one quantisation step
    } else {
        mapper_report.max_abs_error + 1e-12 // same-impl: no worse than our own mapper
    };
    let pass = recovered.len() == total && max_abs <= bound;

    let mode = if interop { "interop (<=1 LSB)" } else { "strict (<=mapper loss)" };
    println!("IMPORT samples={total} packets_ok  mode={mode}");
    println!("  documented mapping: {EXPORT_BIT_DEPTH}-bit signed, full_scale={EXPORT_FULL_SCALE}, 1 LSB={one_lsb:.3e}");
    println!("  mapper-only loss:  max_abs={:.3e} rms={:.3e}", mapper_report.max_abs_error, mapper_report.rms_error);
    println!("  end-to-end loss:   max_abs={max_abs:.3e} rms={rms:.3e}");
    if pass {
        println!("IMPORT OK — recovered within documented quantization loss");
        Ok(())
    } else {
        Err(format!(
            "end-to-end loss {max_abs:.3e} exceeds bound {bound:.3e} ({mode}), or sample count mismatch ({}/{total})",
            recovered.len()
        ))
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let usage = || {
        eprintln!(
            "usage: difi-tool <gen-golden <dir> | dump <file> | map-check <depth> \
| send <host:port> <count> | recv <bind> <count> \
| tcp-send <host:port> <count> | tcp-recv <bind> <count> \
| export <host:port> | import <bind> | import-interop <bind> | gen-check \
| generate <work-dir> | verify-manifest <file>>"
        )
    };
    match args.get(1).map(String::as_str) {
        Some("gen-golden") => match args.get(2) {
            Some(dir) => match write_golden(dir) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            None => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("dump") => match args.get(2) {
            Some(file) => match dump(file) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            None => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("send") => match (args.get(2), args.get(3).and_then(|s| s.parse().ok())) {
            (Some(dest), Some(count)) => match send(dest, count) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            _ => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("recv") => match (args.get(2), args.get(3).and_then(|s| s.parse().ok())) {
            (Some(bind), Some(count)) => match recv(bind, count) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            _ => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("tcp-send") => match (args.get(2), args.get(3).and_then(|s| s.parse().ok())) {
            (Some(dest), Some(count)) => match tcp_send(dest, count) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            _ => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("tcp-recv") => match (args.get(2), args.get(3).and_then(|s| s.parse().ok())) {
            (Some(bind), Some(count)) => match tcp_recv(bind, count) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            _ => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("map-check") => match args.get(2).and_then(|s| s.parse().ok()) {
            Some(depth) => match map_check(depth) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            None => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("export") => match args.get(2) {
            Some(dest) => match export(dest) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            None => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("import") => match args.get(2) {
            Some(bind) => match import(bind) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            None => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("import-interop") => match args.get(2) {
            Some(bind) => match import_interop(bind) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            None => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("gen-check") => match gen_check() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Some("generate") => match args.get(2) {
            Some(dir) => match generate_to_dir(dir) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            None => {
                usage();
                ExitCode::FAILURE
            }
        },
        Some("verify-manifest") => match args.get(2) {
            Some(path) => match verify_manifest(path) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            },
            None => {
                usage();
                ExitCode::FAILURE
            }
        },
        _ => {
            usage();
            ExitCode::FAILURE
        }
    }
}
