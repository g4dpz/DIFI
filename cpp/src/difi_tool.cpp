// SPDX-License-Identifier: Apache-2.0
// difi-tool — minimal CLI for cross-implementation interop (C++ side).
//
// Subcommands mirror the Rust tool exactly so the two implementations can exchange
// packets and be diffed byte-for-byte:
//   gen-golden <dir>   Write the canonical golden packets to <dir>.
//   dump <file>        Decode a raw packet file and print its fields as text.

#include <algorithm>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <string>
#include <vector>

#include "difi/difi.hpp"

namespace {

constexpr uint32_t STREAM_ID = 0x12345678;
constexpr uint32_t INT_SECONDS = 1700000000u;
constexpr uint64_t FRAC_PS = 123456789000ull;

constexpr uint8_t EXPORT_BIT_DEPTH = 16;
constexpr double EXPORT_FULL_SCALE = 1.0;
constexpr size_t EXPORT_SAMPLES_PER_PACKET = 256;

// Deterministic Timing Flow Control golden (mirrors the Rust golden_flow_control so the
// two tools produce byte-identical output).
difi::TimingFlowControlPacket golden_flow_control(difi::FlowControlKind kind) {
  namespace c = difi::constants;
  difi::TimingFlowControlPacket p{};
  p.header.packet_type = c::packet_type::COMMAND;
  p.header.class_id_present = true;
  p.header.tsm = c::tsm::DATA;
  p.header.tsi = c::tsi::UTC;
  p.header.tsf = (kind == difi::FlowControlKind::SampleCount) ? c::tsf::SAMPLE_COUNT
                                                              : c::tsf::REAL_TIME_PS;
  p.header.seq_num = 0;
  p.header.stream_id = STREAM_ID;
  // Info Class pairs with the flow-control Packet Class per the v1.2.1 reference:
  // 0x0002 <-> 0x0005 (sample count), 0x0003 <-> 0x0006 (real-time TSF).
  p.header.information_class =
      (kind == difi::FlowControlKind::SampleCount)
          ? c::information_class::DATA_PLANE_FLOW_CONTROL
          : c::information_class::DATA_PLANE_FLOW_CONTROL_RT;
  p.header.packet_class = difi::flow_control_packet_class(kind);
  p.header.integer_seconds = INT_SECONDS;
  p.header.fractional_seconds =
      (kind == difi::FlowControlKind::SampleCount) ? 4096ull : FRAC_PS;
  p.kind = kind;
  p.cam.controllee_id_present = true;
  p.cam.controller_id_present = true;
  p.cam.action_mode = difi::command::action_mode::EXECUTE;
  p.message_id = 1;
  p.controllee_id = 0x000000A1;
  p.controller_id = 0x000000B2;
  p.reference_point = c::command::REF_POINT;
  p.sample_rate_hz = 1.25e6;
  p.timestamp_adjust_fs = 2500;
  p.buffer.size_bytes = 1u << 20;
  p.buffer.near_full = true;
  p.buffer.level = 0x0300;  // 12-bit fill level (bits 15-4 of the status word)
  return p;
}

std::vector<difi::IqSample> golden_samples() {
  std::vector<difi::IqSample> v;
  for (int n = 0; n < 8; ++n) v.push_back({n - 4, 100 - n * 25});
  return v;
}

// Must match stream_samples() in the Rust tool exactly so each tool can verify the
// other's live stream sample-for-sample.
std::vector<difi::IqSample> stream_samples(uint32_t packet_index) {
  std::vector<difi::IqSample> v;
  for (uint32_t n = 0; n < 8; ++n) {
    int32_t base = static_cast<int32_t>(packet_index * 8 + n);
    v.push_back({base - 16, 16 - base});
  }
  return v;
}

bool write_file(const std::string& path, const std::vector<uint8_t>& bytes) {
  std::ofstream f(path, std::ios::binary);
  if (!f) return false;
  f.write(reinterpret_cast<const char*>(bytes.data()),
          static_cast<std::streamsize>(bytes.size()));
  return static_cast<bool>(f);
}

std::vector<uint8_t> read_file(const std::string& path) {
  std::ifstream f(path, std::ios::binary);
  if (!f) throw difi::DifiError("cannot open " + path);
  return std::vector<uint8_t>((std::istreambuf_iterator<char>(f)),
                              std::istreambuf_iterator<char>());
}

int gen_golden(const std::string& dir) {
  std::error_code ec;
  std::filesystem::create_directories(dir, ec);
  difi::StreamEncoder enc(STREAM_ID, 16);
  auto ctx = enc.encode_context(2.401e9, 1.0e6, 1.25e6, -3.5, INT_SECONDS, FRAC_PS);
  auto ver = enc.encode_version(2021, 100, INT_SECONDS, FRAC_PS);
  auto data = enc.encode_data(golden_samples(), INT_SECONDS, FRAC_PS);

  // DIFI 1.2 goldens.
  auto sc_ctx = enc.encode_context_sample_count(2.401e9, 1.0e6, 1.25e6, -3.5, INT_SECONDS, 0);
  auto sc_data = enc.encode_data_sample_count(golden_samples(), INT_SECONDS, 4096);
  auto tfc_sc = golden_flow_control(difi::FlowControlKind::SampleCount).pack();
  auto tfc_rt = golden_flow_control(difi::FlowControlKind::RealTime).pack();

  if (!write_file(dir + "/signal_context_16bit.difi.bin", ctx) ||
      !write_file(dir + "/version_context.difi.bin", ver) ||
      !write_file(dir + "/signal_data_16bit.difi.bin", data) ||
      !write_file(dir + "/sample_count_context_16bit.difi.bin", sc_ctx) ||
      !write_file(dir + "/sample_count_data_16bit.difi.bin", sc_data) ||
      !write_file(dir + "/timing_flow_control_sc.difi.bin", tfc_sc) ||
      !write_file(dir + "/timing_flow_control_rt.difi.bin", tfc_rt)) {
    std::fprintf(stderr, "error: failed to write golden packets to %s\n", dir.c_str());
    return 1;
  }
  std::printf("wrote 7 golden packets to %s\n", dir.c_str());
  return 0;
}

int dump(const std::string& file) {
  auto buf = read_file(file);
  difi::Decoder dec;
  dec.set_bit_depth(STREAM_ID, 16);
  uint32_t sid;
  if (difi::peek_stream_id(buf, sid)) dec.set_bit_depth(sid, 16);
  auto pkt = dec.decode(buf);
  switch (pkt.kind) {
    case difi::PacketKind::SignalContext: {
      const auto& c = *pkt.context;
      std::printf("SignalContext stream=0x%08X class=0x0001\n", c.header.stream_id);
      std::printf("  rf=%.0f Hz bw=%.0f Hz fs=%.0f Hz gain=%g dB bit_depth=%u\n",
                  c.rf_reference_hz, c.bandwidth_hz, c.sample_rate_hz,
                  static_cast<double>(c.gain_db), c.data_format.bit_depth);
      break;
    }
    case difi::PacketKind::VersionContext: {
      const auto& v = *pkt.version;
      std::printf("VersionContext stream=0x%08X\n", v.header.stream_id);
      std::printf("  year=%u day=%u changed=%s\n", v.year, v.day,
                  v.context_changed ? "true" : "false");
      break;
    }
    case difi::PacketKind::SignalData: {
      const auto& d = *pkt.data;
      std::printf("SignalData stream=0x%08X seq=%u bit_depth=%u samples=%zu\n",
                  d.header.stream_id, d.header.seq_num, d.bit_depth, d.samples.size());
      for (size_t n = 0; n < d.samples.size(); ++n)
        std::printf("  [%zu] I=%d Q=%d\n", n, d.samples[n].i, d.samples[n].q);
      break;
    }
    case difi::PacketKind::TimingFlowControl: {
      const auto& f = *pkt.flow_control;
      const char* kind =
          f.kind == difi::FlowControlKind::SampleCount ? "SampleCount" : "RealTime";
      auto b = [](bool v) { return v ? "true" : "false"; };
      std::printf("TimingFlowControl stream=0x%08X class=0x%04X kind=%s\n",
                  f.header.stream_id, f.header.packet_class, kind);
      std::printf("  controllee=0x%08X controller=0x%08X msg_id=%u\n", f.controllee_id,
                  f.controller_id, f.message_id);
      std::printf("  fs=%.0f Hz ts_adj=%lld fs buffer=%llu B OF=%s NF=%s NE=%s UF=%s\n",
                  f.sample_rate_hz, static_cast<long long>(f.timestamp_adjust_fs),
                  static_cast<unsigned long long>(f.buffer.size_bytes),
                  b(f.buffer.overflow), b(f.buffer.near_full), b(f.buffer.near_empty),
                  b(f.buffer.underflow));
      break;
    }
  }
  return 0;
}

// Must match map_check_vector() in the Rust tool exactly.
std::vector<std::pair<float, float>> map_check_vector() {
  return {
      {0.0f, 0.0f},
      {0.5f, -0.5f},
      {1.0f, -1.0f},
      {1.5f, -1.5f},
      {-1.5f, 1.5f},
      {0.123456f, -0.987654f},
      {0.0000305f, -0.0000305f},
      {0.99999f, -0.99999f},
  };
}

int map_check(uint8_t bit_depth) {
  difi::ScalingConvention conv(bit_depth, 1.0);
  auto v = map_check_vector();
  auto codes = difi::float_to_difi(v, conv);
  std::printf("bit_depth=%u fs_code=%d\n", bit_depth, conv.fs_code());
  for (const auto& s : codes) std::printf("%d %d\n", s.i, s.q);
  auto report = difi::measure_implementation_loss(v, conv);
  std::printf("loss max_abs=%.9f rms=%.9f\n", report.max_abs_error, report.rms_error);
  return 0;
}

constexpr const char* REFERENCE_SIGNAL_NAME = "afs-style-tone-4096";

// Minimal JSON field readers for our own stable manifest format.
static std::string json_string_field(const std::string& t, const std::string& key) {
  auto n = t.find("\"" + key + "\"");
  if (n == std::string::npos) return "";
  auto colon = t.find(':', n);
  auto q1 = t.find('"', colon + 1);
  auto q2 = t.find('"', q1 + 1);
  if (q1 == std::string::npos || q2 == std::string::npos) return "";
  return t.substr(q1 + 1, q2 - q1 - 1);
}
static long json_number_field(const std::string& t, const std::string& key) {
  auto n = t.find("\"" + key + "\"");
  if (n == std::string::npos) return -1;
  auto colon = t.find(':', n);
  return std::stol(t.substr(colon + 1));
}

// Generate the reference signal: raw cf32 to <work-dir> (gitignored), manifest to the
// tracked signals/ dir. Byte-identical to the Rust tool (same generator + encoding).
int generate_signal_cmd(const std::string& work_dir) {
  difi::SignalParams params;
  auto iq = difi::generate_signal(params);
  auto raw = difi::encode_cf32(iq);
  std::string sha = difi::sha256_hex(raw);

  std::error_code ec;
  std::filesystem::create_directories(work_dir, ec);
  std::string raw_path = work_dir + "/" + REFERENCE_SIGNAL_NAME + ".cf32";
  if (!write_file(raw_path, raw)) {
    std::fprintf(stderr, "error: cannot write %s\n", raw_path.c_str());
    return 1;
  }
  std::filesystem::create_directories("signals", ec);
  std::string manifest_path =
      std::string("signals/") + REFERENCE_SIGNAL_NAME + ".manifest.json";

  char buf[1200];
  std::snprintf(
      buf, sizeof(buf),
      "{\n  \"name\": \"%s\",\n  \"raw_format\": \"cf32\",\n  \"byte_size\": %zu,\n"
      "  \"sha256\": \"%s\",\n  \"params\": {\n    \"num_samples\": %zu,\n"
      "    \"sample_rate_hz\": %lld,\n    \"tone1_hz\": %lld,\n    \"tone2_hz\": %lld,\n"
      "    \"peak\": %g\n  },\n  \"difi\": {\n    \"bit_depth\": %u,\n"
      "    \"full_scale\": %lld,\n    \"rf_reference_hz\": %lld,\n"
      "    \"bandwidth_hz\": %lld,\n    \"sample_rate_hz\": %lld,\n    \"gain_db\": %g\n  },\n"
      "  \"regen_command\": \"difi-tool generate <work-dir> (writes %s.cf32)\"\n}\n",
      REFERENCE_SIGNAL_NAME, raw.size(), sha.c_str(), params.num_samples,
      (long long)params.sample_rate_hz, (long long)params.tone1_hz,
      (long long)params.tone2_hz, (double)params.peak, EXPORT_BIT_DEPTH,
      (long long)EXPORT_FULL_SCALE, (long long)2401000000LL,
      (long long)params.sample_rate_hz, (long long)params.sample_rate_hz, -3.5,
      REFERENCE_SIGNAL_NAME);
  std::ofstream mf(manifest_path);
  mf << buf;
  mf.close();

  std::printf("generated %zu samples\n", iq.size());
  std::printf("  raw      %s (%zu bytes, gitignored)\n", raw_path.c_str(), raw.size());
  std::printf("  manifest %s (committed)\n", manifest_path.c_str());
  std::printf("  sha256   %s\n", sha.c_str());
  return 0;
}

int verify_manifest_cmd(const std::string& manifest_path) {
  auto bytes = read_file(manifest_path);
  std::string text(bytes.begin(), bytes.end());
  std::string recorded_sha = json_string_field(text, "sha256");
  long recorded_size = json_number_field(text, "byte_size");

  difi::SignalParams params;
  auto iq = difi::generate_signal(params);
  auto raw = difi::encode_cf32(iq);
  std::string actual_sha = difi::sha256_hex(raw);

  std::printf("verifying %s\n", manifest_path.c_str());
  std::printf("  recorded size=%ld sha256=%s\n", recorded_size, recorded_sha.c_str());
  std::printf("  regen    size=%zu sha256=%s\n", raw.size(), actual_sha.c_str());
  if (static_cast<long>(raw.size()) == recorded_size && actual_sha == recorded_sha) {
    std::printf("MANIFEST OK — regenerated signal matches recorded SHA-256\n");
    return 0;
  }
  std::fprintf(stderr, "error: regenerated signal does not match the manifest\n");
  return 1;
}

// Print the DIFI integer codes of the generated reference signal, so the two
// implementations' generated+mapped output can be diffed for exact equality.
int gen_check() {
  difi::SignalParams params;
  auto float_iq = difi::generate_signal(params);
  difi::ScalingConvention conv(EXPORT_BIT_DEPTH, EXPORT_FULL_SCALE);
  auto codes = difi::float_to_difi(float_iq, conv);
  std::printf("samples=%zu bit_depth=%u\n", codes.size(), EXPORT_BIT_DEPTH);
  for (auto& s : codes) std::printf("%d %d\n", s.i, s.q);
  return 0;
}

int export_signal(const std::string& dest) {
  difi::SignalParams params;
  auto float_iq = difi::generate_signal(params);
  difi::ScalingConvention conv(EXPORT_BIT_DEPTH, EXPORT_FULL_SCALE);
  auto codes = difi::float_to_difi(float_iq, conv);

  difi::StreamEncoder enc(STREAM_ID, EXPORT_BIT_DEPTH);
  difi::UdpSender tx(dest);
  auto ctx = enc.encode_context(2.401e9, params.sample_rate_hz, params.sample_rate_hz,
                                -3.5, INT_SECONDS, FRAC_PS);
  tx.send(ctx);

  size_t n_packets = 0;
  for (size_t off = 0; off < codes.size(); off += EXPORT_SAMPLES_PER_PACKET) {
    size_t end = std::min(off + EXPORT_SAMPLES_PER_PACKET, codes.size());
    std::vector<difi::IqSample> chunk(codes.begin() + off, codes.begin() + end);
    auto data = enc.encode_data(chunk, INT_SECONDS, FRAC_PS);
    tx.send(data);
    usleep(2000);
    ++n_packets;
  }
  std::printf("exported %zu samples in %zu data packets (+1 context) to %s\n",
              codes.size(), n_packets, dest.c_str());
  return 0;
}

int import_signal(const std::string& bind_addr, bool interop = false) {
  difi::SignalParams params;
  auto original = difi::generate_signal(params);
  size_t total = original.size();
  difi::ScalingConvention conv(EXPORT_BIT_DEPTH, EXPORT_FULL_SCALE);
  auto mapper_report = difi::measure_implementation_loss(original, conv);

  difi::UdpReceiver rx(bind_addr);
  rx.set_stream_filter(true, STREAM_ID);
  rx.set_read_timeout_ms(15000);
  difi::Decoder dec;

  std::vector<difi::IqSample> recovered_codes;
  recovered_codes.reserve(total);
  bool ctx_seen = false;
  while (recovered_codes.size() < total) {
    auto buf = rx.recv();
    auto pkt = dec.decode(buf);
    if (pkt.kind == difi::PacketKind::SignalContext) {
      ctx_seen = true;
    } else if (pkt.kind == difi::PacketKind::SignalData) {
      for (auto& s : pkt.data->samples) recovered_codes.push_back(s);
    }
  }
  if (!ctx_seen) {
    std::fprintf(stderr, "error: no Signal Context received before data\n");
    return 1;
  }

  auto recovered = difi::difi_to_float(recovered_codes, conv);
  double sum_sq_err = 0.0, max_abs = 0.0;
  for (size_t k = 0; k < total; ++k) {
    const double origs[2] = {original[k].first, original[k].second};
    const double recs[2] = {recovered[k].first, recovered[k].second};
    for (int c = 0; c < 2; ++c) {
      double e = recs[c] - origs[c];
      sum_sq_err += e * e;
      if (std::fabs(e) > max_abs) max_abs = std::fabs(e);
    }
  }
  double rms = std::sqrt(sum_sq_err / (total * 2));
  double one_lsb = EXPORT_FULL_SCALE / conv.fs_code();
  // strict (same-impl): no worse than our own mapper loss. interop (cross-stack):
  // within one quantisation step (a peer's float math may differ by a ULP).
  double bound = interop ? one_lsb : (mapper_report.max_abs_error + 1e-12);
  bool pass = recovered.size() == total && max_abs <= bound;

  std::printf("IMPORT samples=%zu packets_ok  mode=%s\n", total,
              interop ? "interop (<=1 LSB)" : "strict (<=mapper loss)");
  std::printf("  documented mapping: %u-bit signed, full_scale=%g, 1 LSB=%.3e\n",
              EXPORT_BIT_DEPTH, EXPORT_FULL_SCALE, one_lsb);
  std::printf("  mapper-only loss:  max_abs=%.3e rms=%.3e\n",
              mapper_report.max_abs_error, mapper_report.rms_error);
  std::printf("  end-to-end loss:   max_abs=%.3e rms=%.3e\n", max_abs, rms);
  if (pass) {
    std::printf("IMPORT OK — recovered within documented quantization loss\n");
    return 0;
  }
  std::fprintf(stderr, "error: end-to-end loss exceeds documented bound\n");
  return 1;
}

int send_stream(const std::string& dest, uint32_t count) {
  difi::StreamEncoder enc(STREAM_ID, 16);
  difi::UdpSender tx(dest);
  auto ctx = enc.encode_context(2.401e9, 1.0e6, 1.25e6, -3.5f, INT_SECONDS, FRAC_PS);
  tx.send(ctx);
  for (uint32_t p = 0; p < count; ++p) {
    auto data = enc.encode_data(stream_samples(p), INT_SECONDS, FRAC_PS + p);
    tx.send(data);
    usleep(2000);  // 2 ms gap to avoid localhost socket-buffer overflow on bursts
  }
  std::printf("sent 1 context + %u data packets to %s (stream 0x%08X)\n", count,
              dest.c_str(), STREAM_ID);
  return 0;
}

// NON-STANDARD TCP dev transport (Req 5.2): one context + count data packets.
int tcp_send_stream(const std::string& dest, uint32_t count) {
  difi::StreamEncoder enc(STREAM_ID, 16);
  difi::TcpSender tx(dest);
  tx.send(enc.encode_context(2.401e9, 1.0e6, 1.25e6, -3.5, INT_SECONDS, FRAC_PS));
  for (uint32_t p = 0; p < count; ++p)
    tx.send(enc.encode_data(stream_samples(p), INT_SECONDS, FRAC_PS + p));
  std::printf("tcp-sent 1 context + %u data packets to %s (stream 0x%08X)\n", count,
              dest.c_str(), STREAM_ID);
  return 0;
}

int tcp_recv_stream(const std::string& bind_addr, uint32_t count) {
  difi::TcpReceiver rx(bind_addr);
  rx.set_stream_filter(true, STREAM_ID);
  difi::Decoder dec;
  uint32_t data_seen = 0, ctx_seen = 0;
  while (data_seen < count) {
    auto buf = rx.recv();
    if (buf.empty()) {
      std::fprintf(stderr, "error: stream closed after %u/%u data packets\n", data_seen,
                   count);
      return 1;
    }
    auto pkt = dec.decode(buf);
    if (pkt.kind == difi::PacketKind::SignalContext) {
      ++ctx_seen;
    } else if (pkt.kind == difi::PacketKind::SignalData) {
      auto expected = stream_samples(data_seen);
      if (pkt.data->samples != expected) {
        std::fprintf(stderr, "error: sample mismatch at data packet %u\n", data_seen);
        return 1;
      }
      if (pkt.data->header.seq_num != static_cast<uint8_t>(data_seen % 16)) {
        std::fprintf(stderr, "error: seq mismatch at packet %u\n", data_seen);
        return 1;
      }
      ++data_seen;
    }
  }
  if (ctx_seen == 0) {
    std::fprintf(stderr, "error: no Signal Context received before data\n");
    return 1;
  }
  std::printf("RECV OK %u\n", data_seen);
  return 0;
}

int recv_stream(const std::string& bind_addr, uint32_t count) {
  difi::UdpReceiver rx(bind_addr);
  rx.set_stream_filter(true, STREAM_ID);
  rx.set_read_timeout_ms(10000);
  difi::Decoder dec;

  uint32_t data_seen = 0, ctx_seen = 0;
  while (data_seen < count) {
    auto buf = rx.recv();
    auto pkt = dec.decode(buf);
    if (pkt.kind == difi::PacketKind::SignalContext) {
      ++ctx_seen;
      if (pkt.context->data_format.bit_depth != 16) {
        std::fprintf(stderr, "error: unexpected bit depth\n");
        return 1;
      }
    } else if (pkt.kind == difi::PacketKind::SignalData) {
      auto expected = stream_samples(data_seen);
      if (pkt.data->samples != expected) {
        std::fprintf(stderr, "error: sample mismatch at data packet %u\n", data_seen);
        return 1;
      }
      if (pkt.data->header.seq_num != static_cast<uint8_t>(data_seen % 16)) {
        std::fprintf(stderr, "error: seq mismatch at packet %u (got %u)\n", data_seen,
                     pkt.data->header.seq_num);
        return 1;
      }
      ++data_seen;
    }
  }
  if (ctx_seen == 0) {
    std::fprintf(stderr, "error: no Signal Context received before data\n");
    return 1;
  }
  std::printf("RECV OK %u\n", data_seen);
  return 0;
}

// One deterministic line describing a decoded Timing Flow Control packet — identical
// format to the Rust fc_line so the two implementations' output can be compared.
std::string fc_line(const difi::TimingFlowControlPacket& f) {
  const char* kind =
      f.kind == difi::FlowControlKind::SampleCount ? "SampleCount" : "RealTime";
  auto b = [](bool v) { return v ? "true" : "false"; };
  char out[256];
  std::snprintf(out, sizeof(out),
                "FC class=0x%04X kind=%s msgid=%u controllee=0x%08X controller=0x%08X "
                "fs=%.0f ts_adj=%lld buf=%llu OF=%s NF=%s NE=%s UF=%s",
                f.header.packet_class, kind, f.message_id, f.controllee_id,
                f.controller_id, f.sample_rate_hz,
                static_cast<long long>(f.timestamp_adjust_fs),
                static_cast<unsigned long long>(f.buffer.size_bytes), b(f.buffer.overflow),
                b(f.buffer.near_full), b(f.buffer.near_empty), b(f.buffer.underflow));
  return out;
}

// Flow-control SOURCE: stream count data packets to data_dest, then receive the Sink's
// Timing Flow Control packet on fc_bind and print the reported buffer state.
int fc_source(const std::string& data_dest, const std::string& fc_bind, uint32_t count) {
  difi::UdpReceiver fc_rx(fc_bind);  // bind first so the reply can't race ahead
  fc_rx.set_stream_filter(true, STREAM_ID);
  fc_rx.set_read_timeout_ms(10000);

  difi::StreamEncoder enc(STREAM_ID, 16);
  difi::UdpSender tx(data_dest);
  tx.send(enc.encode_context(2.401e9, 1.0e6, 1.25e6, -3.5, INT_SECONDS, FRAC_PS));
  for (uint32_t p = 0; p < count; ++p) {
    tx.send(enc.encode_data(stream_samples(p), INT_SECONDS, FRAC_PS + p));
    usleep(2000);
  }

  difi::Decoder dec;
  for (;;) {
    auto buf = fc_rx.recv();
    auto pkt = dec.decode(buf);
    if (pkt.kind == difi::PacketKind::TimingFlowControl) {
      std::printf("%s\n", fc_line(*pkt.flow_control).c_str());
      return 0;
    }
  }
}

// Flow-control SINK: receive count data packets on data_bind, then emit a deterministic
// Timing Flow Control packet to fc_dest reporting buffer state.
int fc_sink(const std::string& data_bind, const std::string& fc_dest, uint32_t count) {
  difi::UdpReceiver rx(data_bind);
  rx.set_stream_filter(true, STREAM_ID);
  rx.set_read_timeout_ms(10000);
  difi::Decoder dec;

  uint32_t data_seen = 0;
  while (data_seen < count) {
    auto buf = rx.recv();
    if (dec.decode(buf).kind == difi::PacketKind::SignalData) ++data_seen;
  }

  auto fc = golden_flow_control(difi::FlowControlKind::RealTime);
  difi::UdpSender tx(fc_dest);
  tx.send(fc.pack());
  std::printf("%s\n", fc_line(fc).c_str());
  return 0;
}

}  // namespace

int main(int argc, char** argv) {
  auto usage = []() {
    std::fprintf(stderr,
                 "usage: difi-tool <gen-golden <dir> | dump <file> | "
                 "send <host:port> <count> | recv <bind> <count>>\n");
  };
  try {
    if (argc >= 3 && std::string(argv[1]) == "gen-golden") return gen_golden(argv[2]);
    if (argc >= 3 && std::string(argv[1]) == "dump") return dump(argv[2]);
    if (argc >= 4 && std::string(argv[1]) == "send")
      return send_stream(argv[2], static_cast<uint32_t>(std::stoul(argv[3])));
    if (argc >= 4 && std::string(argv[1]) == "recv")
      return recv_stream(argv[2], static_cast<uint32_t>(std::stoul(argv[3])));
    if (argc >= 5 && std::string(argv[1]) == "fc-source")
      return fc_source(argv[2], argv[3], static_cast<uint32_t>(std::stoul(argv[4])));
    if (argc >= 5 && std::string(argv[1]) == "fc-sink")
      return fc_sink(argv[2], argv[3], static_cast<uint32_t>(std::stoul(argv[4])));
    if (argc >= 4 && std::string(argv[1]) == "tcp-send")
      return tcp_send_stream(argv[2], static_cast<uint32_t>(std::stoul(argv[3])));
    if (argc >= 4 && std::string(argv[1]) == "tcp-recv")
      return tcp_recv_stream(argv[2], static_cast<uint32_t>(std::stoul(argv[3])));
    if (argc >= 3 && std::string(argv[1]) == "map-check")
      return map_check(static_cast<uint8_t>(std::stoul(argv[2])));
    if (argc >= 3 && std::string(argv[1]) == "export") return export_signal(argv[2]);
    if (argc >= 3 && std::string(argv[1]) == "import") return import_signal(argv[2]);
    if (argc >= 3 && std::string(argv[1]) == "import-interop")
      return import_signal(argv[2], true);
    if (argc >= 2 && std::string(argv[1]) == "gen-check") return gen_check();
    if (argc >= 3 && std::string(argv[1]) == "generate")
      return generate_signal_cmd(argv[2]);
    if (argc >= 3 && std::string(argv[1]) == "verify-manifest")
      return verify_manifest_cmd(argv[2]);
    usage();
    return 1;
  } catch (const std::exception& e) {
    std::fprintf(stderr, "error: %s\n", e.what());
    return 1;
  }
}
