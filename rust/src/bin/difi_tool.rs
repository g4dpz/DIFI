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
    // Golden data uses 16-bit; register it so standalone data packets decode.
    dec.set_bit_depth(STREAM_ID, 16);
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
            DifiPacket::VersionContext(_) => {}
        }
    }
    if ctx_seen == 0 {
        return Err("no Signal Context packet received before data".into());
    }
    println!("RECV OK {data_seen}");
    Ok(())
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
fn import(bind_addr: &str) -> Result<(), String> {
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

    let mut recovered_codes: Vec<IqSample> = Vec::with_capacity(total);
    let mut ctx_seen = false;
    while recovered_codes.len() < total {
        let (buf, _from) = rx.recv().map_err(|e| format!("recv: {e}"))?;
        match dec.decode(&buf).map_err(|e| e.to_string())? {
            DifiPacket::SignalContext(_) => ctx_seen = true,
            DifiPacket::SignalData(d) => recovered_codes.extend(d.samples),
            DifiPacket::VersionContext(_) => {}
        }
    }
    if !ctx_seen {
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

    // PASS criteria: all samples recovered, and end-to-end loss does not exceed the
    // mapper-only quantisation bound (transport/codec added nothing).
    let one_lsb = EXPORT_FULL_SCALE / conv.fs_code() as f64;
    let pass = recovered.len() == total
        && max_abs <= mapper_report.max_abs_error + 1e-12
        && max_abs <= one_lsb; // within one quantisation step

    println!("IMPORT samples={total} packets_ok");
    println!("  documented mapping: {EXPORT_BIT_DEPTH}-bit signed, full_scale={EXPORT_FULL_SCALE}, 1 LSB={one_lsb:.3e}");
    println!("  mapper-only loss:  max_abs={:.3e} rms={:.3e}", mapper_report.max_abs_error, mapper_report.rms_error);
    println!("  end-to-end loss:   max_abs={max_abs:.3e} rms={rms:.3e}");
    if pass {
        println!("IMPORT OK — recovered within documented quantization loss");
        Ok(())
    } else {
        Err(format!(
            "end-to-end loss {max_abs:.3e} exceeds mapper bound {:.3e} / 1 LSB {one_lsb:.3e}, or sample count mismatch ({}/{total})",
            mapper_report.max_abs_error,
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
| export <host:port> | import <bind>>"
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
        Some("gen-check") => match gen_check() {
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
    }
}
