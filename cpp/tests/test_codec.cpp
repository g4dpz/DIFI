// SPDX-License-Identifier: Apache-2.0
// Unit tests for the DIFI 1.1 C++ codec (mirrors rust/tests/codec.rs).
// Minimal dependency-free harness: each CHECK records pass/fail; main returns nonzero
// if any failed.

#include <cmath>
#include <cstdio>
#include <functional>
#include <string>
#include <thread>
#include <vector>

#include "difi/difi.hpp"

static int g_failures = 0;
static int g_checks = 0;

#define CHECK(cond)                                                       \
  do {                                                                    \
    ++g_checks;                                                           \
    if (!(cond)) {                                                        \
      ++g_failures;                                                       \
      std::printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);         \
    }                                                                     \
  } while (0)

static bool throws(const std::function<void()>& fn) {
  try {
    fn();
  } catch (const difi::DifiError&) {
    return true;
  } catch (...) {
    return false;
  }
  return false;
}

using namespace difi;

static Header make_header(uint8_t ptype, uint16_t iclass, uint16_t pclass, uint8_t seq) {
  Header h{};
  h.packet_type = ptype;
  h.class_id_present = true;
  h.tsm = (ptype == constants::packet_type::SIGNAL_DATA) ? constants::tsm::DATA
                                                         : constants::tsm::CONTEXT;
  h.tsi = constants::tsi::UTC;
  h.tsf = constants::tsf::REAL_TIME_PS;
  h.seq_num = seq;
  h.stream_id = 0xDEADBEEF;
  h.information_class = iclass;
  h.packet_class = pclass;
  h.integer_seconds = 1700000000u;
  h.fractional_seconds = 42000000000ull;
  return h;
}

static void test_iq_round_trip() {
  for (uint8_t depth : {4, 5, 7, 8, 11, 12, 15, 16}) {
    auto [min, max] = sample_range(depth);
    std::vector<IqSample> samples;
    for (int k = 0; k < 8; ++k) samples.push_back({min, max});
    for (int k = 0; k < 8; ++k) samples.push_back({max, min});
    auto packed = pack_samples(samples, depth);
    CHECK(packed.size() % 4 == 0);
    auto unpacked = unpack_samples(packed, 0, samples.size(), depth);
    CHECK(unpacked == samples);
  }
}

static void test_iq_rejections() {
  CHECK(throws([] { pack_samples({{0, 0}}, 3); }));
  CHECK(throws([] { pack_samples({{0, 0}}, 17); }));
  CHECK(throws([] { payload_word_count(1, 12); }));  // 24 bits, not word aligned
  const int32_t mx = sample_range(8).second;
  CHECK(throws([mx] {
    std::vector<IqSample> s(16, {mx + 1, 0});
    pack_samples(s, 8);
  }));
}

static void test_signal_data_round_trip() {
  std::vector<IqSample> samples;
  for (int k = 0; k < 16; ++k) samples.push_back({k - 8, 8 - k});
  SignalDataPacket pkt{
      make_header(constants::packet_type::SIGNAL_DATA,
                  constants::information_class::STANDARD_FLOW,
                  constants::packet_class::SIGNAL_DATA, 5),
      16, samples};
  auto bytes = pkt.pack();
  CHECK(bytes.size() % 4 == 0);
  auto decoded = SignalDataPacket::unpack(bytes, 16);
  CHECK(decoded.header == pkt.header);
  CHECK(decoded.samples == samples);
}

static SignalContextPacket sample_context(double rf, double bw, double fs, double gain,
                                          uint8_t bit_depth) {
  return SignalContextPacket{
      make_header(constants::packet_type::CONTEXT,
                  constants::information_class::STANDARD_FLOW,
                  constants::packet_class::SIGNAL_CONTEXT, 0),
      true, rf, 0.0, bw, fs, gain, DataFormat{bit_depth}};
}

static void test_context_round_trips() {
  SignalContextPacket sc = sample_context(2.401e9, 1.0e6, 1.25e6, -3.5, 16);
  auto sb = sc.pack();
  CHECK(sb.size() == 27u * 4);
  auto sd = SignalContextPacket::unpack(sb);
  CHECK(sd.header == sc.header);
  // Fixed-point scaling: compare within tolerance.
  CHECK(std::fabs(sd.rf_reference_hz - sc.rf_reference_hz) < 1.0);
  CHECK(std::fabs(sd.bandwidth_hz - sc.bandwidth_hz) < 1.0);
  CHECK(std::fabs(sd.sample_rate_hz - sc.sample_rate_hz) < 1.0);
  CHECK(std::fabs(sd.gain_db - sc.gain_db) < 0.01);
  CHECK(sd.data_format == sc.data_format);

  VersionContextPacket vc{
      make_header(constants::version_packet_type::VERSION_CONTEXT,
                  constants::information_class::VERSION_FLOW,
                  constants::packet_class::VERSION_CONTEXT, 0),
      true, 2021, 100};
  auto vb = vc.pack();
  CHECK(vb.size() == 11u * 4);
  auto vd = VersionContextPacket::unpack(vb);
  CHECK(vd == vc);
}

static void test_decoder_fail_closed() {
  Decoder dec;
  SignalContextPacket sc = sample_context(1.0, 1.0, 1.0, 0.0, 16);
  auto good = sc.pack();
  // Corrupt OUI (byte 9).
  auto bad = good;
  bad[9] = 0xFF;
  CHECK(throws([&] { dec.decode(bad); }));
  // Truncated.
  CHECK(throws([&] {
    std::vector<uint8_t> tiny(10, 0);
    dec.decode(tiny);
  }));
  // Size mismatch (extra trailing word).
  auto big = good;
  big.insert(big.end(), {0, 0, 0, 0});
  CHECK(throws([&] { dec.decode(big); }));
}

static void test_stream_sequencing() {
  StreamEncoder enc(0xABCD0001u, 16);
  std::vector<IqSample> samples(16, {1, -1});
  Decoder dec;
  dec.set_bit_depth(0xABCD0001u, 16);
  for (uint32_t n = 0; n < 20; ++n) {
    auto bytes = enc.encode_data(samples, 0, 0);
    auto pkt = dec.decode(bytes);
    CHECK(pkt.kind == PacketKind::SignalData);
    CHECK(pkt.data->header.seq_num == static_cast<uint8_t>(n % 16));
    CHECK(pkt.data->samples == samples);
  }
}

static void test_stateful_decode_learns_depth() {
  StreamEncoder enc(0x55u, 12);
  auto ctx = enc.encode_context(1e9, 1e6, 1e6, 0.0f, 0, 0);
  std::vector<IqSample> samples(16, {3, -3});
  auto data = enc.encode_data(samples, 0, 0);

  Decoder dec;
  CHECK(throws([&] { dec.decode(data); }));  // data before context
  auto c = dec.decode(ctx);
  CHECK(c.kind == PacketKind::SignalContext);
  auto d = dec.decode(data);
  CHECK(d.kind == PacketKind::SignalData);
  CHECK(d.data->samples == samples);
}

static void test_mapper_endpoints() {
  for (uint8_t depth : {4, 8, 12, 16}) {
    ScalingConvention conv(depth, 1.0);
    int32_t fs = sample_range(depth).second;
    CHECK(conv.float_to_code(0.0f) == 0);
    CHECK(conv.float_to_code(1.0f) == fs);
    CHECK(conv.float_to_code(-1.0f) == -fs);
  }
}

static void test_mapper_clamps() {
  ScalingConvention conv(8, 1.0);
  CHECK(conv.float_to_code(5.0f) == 127);
  CHECK(conv.float_to_code(-5.0f) == -127);
}

static void test_mapper_loss_measured() {
  uint8_t depth = 12;
  ScalingConvention conv(depth, 1.0);
  double fs = sample_range(depth).second;
  double one_lsb = conv.full_scale / fs;

  size_t n = 1024;
  std::vector<std::pair<float, float>> iq;
  for (size_t k = 0; k < n; ++k) {
    double phase = static_cast<double>(k) / n * 2.0 * M_PI;
    iq.push_back({static_cast<float>(std::cos(phase)), static_cast<float>(std::sin(phase))});
  }
  auto report = measure_implementation_loss(iq, conv);
  CHECK(report.sample_count == n);
  CHECK(report.max_abs_error <= one_lsb * 0.75);
  CHECK(report.rms_error > 0.0);
  CHECK(report.snr_db > 60.0);
}

static void test_mapper_rejects_bad_depth() {
  CHECK(throws([] { ScalingConvention(3, 1.0); }));
  CHECK(throws([] { ScalingConvention(17, 1.0); }));
}

// --- Segmentation (Req 2.5) -------------------------------------------------

static void test_segmented_encode_reassembles() {
  uint8_t depth = 16;
  StreamEncoder enc(0, depth);
  size_t per = enc.max_samples_per_packet();
  CHECK((per * 2 * depth) % 32 == 0);  // word aligned
  CHECK(per > 0);
  size_t total = per * 3 + per / 2;
  std::vector<IqSample> samples;
  for (size_t k = 0; k < total; ++k)
    samples.push_back({(int)(k % 100) - 50, 50 - (int)(k % 100)});

  auto packets = enc.encode_data_segmented(samples, 0, 0);
  CHECK(packets.size() == 4);  // 3.5 packets' worth -> 4 segments

  Decoder dec;
  dec.set_bit_depth(0, depth);
  std::vector<IqSample> reassembled;
  for (size_t i = 0; i < packets.size(); ++i) {
    auto pkt = dec.decode(packets[i]);
    CHECK(pkt.kind == PacketKind::SignalData);
    CHECK(pkt.data->header.seq_num == (uint8_t)(i % 16));
    for (auto& s : pkt.data->samples) reassembled.push_back(s);
  }
  CHECK(reassembled == samples);
}

// --- TCP dev-mode transport (Req 5.2) ---------------------------------------

static void test_tcp_dev_mode_exchange() {
  TcpReceiver rx("127.0.0.1:0");
  uint16_t port = rx.local_port();
  std::vector<IqSample> samples;
  for (int k = 0; k < 32; ++k) samples.push_back({k - 16, 16 - k});

  std::thread tx([port, samples]() {
    TcpSender sender("127.0.0.1:" + std::to_string(port));
    StreamEncoder enc(0xFEED0001u, 16);
    sender.send(enc.encode_context(1e9, 1e6, 1e6, 0.0, 0, 0));
    for (int i = 0; i < 5; ++i) sender.send(enc.encode_data(samples, 0, 0));
    // sender dtor closes the connection -> clean EOF
  });

  Decoder dec;
  auto b1 = rx.recv();
  CHECK(!b1.empty());
  CHECK(dec.decode(b1).kind == PacketKind::SignalContext);
  int data_seen = 0;
  for (;;) {
    auto buf = rx.recv();
    if (buf.empty()) break;  // clean EOF
    auto pkt = dec.decode(buf);
    CHECK(pkt.kind == PacketKind::SignalData);
    CHECK(pkt.data->samples == samples);
    ++data_seen;
  }
  CHECK(data_seen == 5);
  tx.join();
}

// --- Property + fuzz hardening (spec task 14) -------------------------------

// Deterministic xorshift64* PRNG, matching the Rust fuzz harness in spirit.
struct Rng {
  uint64_t s;
  explicit Rng(uint64_t seed) : s(seed | 1) {}
  uint64_t next() {
    uint64_t x = s;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    s = x;
    return x * 0x2545F4914F6CDD1Dull;
  }
  uint64_t below(uint64_t n) { return n ? next() % n : 0; }
  int32_t range_i32(int32_t lo, int32_t hi) {
    uint64_t span = static_cast<uint64_t>(static_cast<int64_t>(hi) - lo + 1);
    return lo + static_cast<int32_t>(below(span));
  }
};

constexpr int FUZZ_ITERS = 20000;

// Property: random valid I/Q round-trips bit-exact at random depths.
static void test_property_iq_round_trip() {
  Rng rng(0xD1F10001ull);
  bool all_ok = true;
  for (int it = 0; it < FUZZ_ITERS; ++it) {
    uint8_t depth = static_cast<uint8_t>(rng.range_i32(4, 16));
    size_t n = 16 * (1 + rng.below(8));  // multiple of 16 keeps word alignment
    auto range = sample_range(depth);
    std::vector<IqSample> samples;
    samples.reserve(n);
    for (size_t k = 0; k < n; ++k)
      samples.push_back({rng.range_i32(range.first, range.second),
                         rng.range_i32(range.first, range.second)});
    auto packed = pack_samples(samples, depth);
    auto unpacked = unpack_samples(packed, 0, n, depth);
    if (unpacked != samples) {
      all_ok = false;
      std::printf("FAIL round-trip depth=%u n=%zu it=%d\n", depth, n, it);
      break;
    }
  }
  CHECK(all_ok);
}

// Fuzz: random garbage buffers must only ever return a packet or throw DifiError,
// never crash or corrupt. Run under ASan/UBSan to catch any out-of-bounds.
static void test_fuzz_decoder_never_crashes() {
  Rng rng(0xF0F0123456789ABCull);
  bool all_ok = true;
  for (int it = 0; it < FUZZ_ITERS; ++it) {
    size_t len = rng.below(80);
    std::vector<uint8_t> buf(len);
    for (auto& b : buf) b = static_cast<uint8_t>(rng.next());
    // Half the time, stamp a valid OUI + plausible pktSize to reach deeper parsing.
    if ((rng.next() & 1) && buf.size() >= 12) {
      uint32_t words = static_cast<uint32_t>(buf.size() / 4);
      uint32_t ptype = static_cast<uint32_t>(rng.below(16)) << 28;
      uint32_t word0 = ptype | (words & 0xFFFF);
      buf[0] = uint8_t(word0 >> 24); buf[1] = uint8_t(word0 >> 16);
      buf[2] = uint8_t(word0 >> 8);  buf[3] = uint8_t(word0);
      buf[8] = 0x00; buf[9] = 0x6A; buf[10] = 0x62; buf[11] = 0x1E;
    }
    uint32_t sid = 0;
    if (buf.size() >= 8)
      sid = (uint32_t(buf[4]) << 24) | (uint32_t(buf[5]) << 16) |
            (uint32_t(buf[6]) << 8) | uint32_t(buf[7]);
    try {
      Decoder dec;
      dec.set_bit_depth(sid, 16);
      auto pkt = dec.decode(buf);
      // Accidentally-valid packet: must be a known kind.
      if (!(pkt.kind == PacketKind::SignalData ||
            pkt.kind == PacketKind::SignalContext ||
            pkt.kind == PacketKind::VersionContext)) {
        all_ok = false;
        std::printf("FAIL: decode returned unknown kind at it=%d\n", it);
        break;
      }
    } catch (const DifiError&) {
      // Expected for malformed input.
    } catch (const std::exception& e) {
      all_ok = false;
      std::printf("FAIL: non-DifiError thrown at it=%d: %s\n", it, e.what());
      break;
    }
  }
  CHECK(all_ok);
}

// Shared Command-Packet core — spec difi-1.2-dataplane task 2. Mirrors the
// rust/src/command.rs unit tests; the two implementations MUST agree bit-for-bit.
static void test_command_core() {
  using namespace difi::command;
  // CAM round-trip preserves all bits, including reserved ones we don't model.
  for (uint32_t word : {0x00000000u, 0xFFFFFFFFu, 0xA1100000u, 0x12345678u, 0x80000000u}) {
    CHECK(Cam::from_word(word).to_word() == word);
  }
  // CAM fields map to the right bits.
  Cam cam{};
  cam.controllee_id_present = true;
  cam.controller_id_present = true;
  cam.action_mode = action_mode::EXECUTE;  // 0b10 -> bit 24
  cam.ack_type = ack_type::VALIDATION;      // 0b100 -> bit 20
  uint32_t w = cam.to_word();
  CHECK(((w >> 31) & 1) == 1);
  CHECK(((w >> 29) & 1) == 1);
  CHECK(((w >> 23) & 0b11) == 0b10);
  CHECK(((w >> 18) & 0b111) == 0b100);
  CHECK(w == (0x80000000u | 0x20000000u | 0x01000000u | 0x00100000u));
  // CIF set/clear.
  Cif c = Cif{}.with(cif0::REFERENCE_POINT, true).with(cif0::SAMPLE_RATE, true).with(
      cif0::CIF1_PRESENT, true);
  CHECK(c.get(cif0::REFERENCE_POINT));
  CHECK(c.get(cif0::SAMPLE_RATE));
  CHECK(c.get(cif0::CIF1_PRESENT));
  CHECK(!c.get(cif0::SHORT_FORM));
  Cif c2 = c.with(cif0::SAMPLE_RATE, false);
  CHECK(!c2.get(cif0::SAMPLE_RATE));
  CHECK(c2.get(cif0::REFERENCE_POINT));
  // Buffer status round-trip.
  BufferState b{};
  b.size_bytes = 1u << 20;
  b.overflow = true;
  b.near_empty = true;
  b.level = 0x12345670;
  uint32_t sw = b.status_word();
  CHECK(((sw >> 3) & 1) == 1);
  CHECK(((sw >> 1) & 1) == 1);
  CHECK(((sw >> 2) & 1) == 0);
  BufferState back = BufferState::from_status_word(b.size_bytes, sw);
  CHECK(back == b);
  // Action-mode validation.
  CHECK(!throws([] { check_action_mode(action_mode::EXECUTE); }));
  CHECK(!throws([] { check_action_mode(0b11); }));
  CHECK(throws([] { check_action_mode(0b100); }));
}

// DIFI 1.2 Sample Count Data/Context twins — spec difi-1.2-dataplane task 4. Mirrors
// rust/tests/sample_count.rs.
static void test_sample_count() {
  // Sample Count Signal Data (0x0002) round-trip with sample-count TSF.
  {
    StreamEncoder enc(0x00000042, 16);
    std::vector<IqSample> samples{{100, -200}, {7, 9}};
    auto bytes = enc.encode_data_sample_count(samples, 1700000000u, 4096);
    Decoder dec;
    dec.set_bit_depth(0x00000042, 16);
    DifiPacket p = dec.decode(bytes);
    CHECK(p.kind == PacketKind::SignalData);
    const auto& d = *p.data;
    CHECK(d.header.packet_class == constants::packet_class::SAMPLE_COUNT_SIGNAL_DATA);
    CHECK(d.header.information_class == constants::information_class::DATA_PLANE_FLOW_CONTROL);
    CHECK(d.header.tsf == constants::tsf::SAMPLE_COUNT);
    CHECK(d.header.fractional_seconds == 4096u);
    CHECK(d.samples == samples);
  }
  // Sample Count Signal Context (0x0003) round-trip; seeds the decoder's bit depth.
  {
    StreamEncoder enc(0x00000043, 12);
    auto bytes = enc.encode_context_sample_count(2.4e9, 20e6, 30.72e6, -3.0, 1700000000u, 0);
    Decoder dec;
    DifiPacket p = dec.decode(bytes);
    CHECK(p.kind == PacketKind::SignalContext);
    const auto& c = *p.context;
    CHECK(c.header.packet_class == constants::packet_class::SAMPLE_COUNT_SIGNAL_CONTEXT);
    CHECK(c.header.tsf == constants::tsf::SAMPLE_COUNT);
    CHECK(c.data_format.bit_depth == 12);
  }
  // Sample-count context is byte-identical to the standard context except word 0 (TSF)
  // and word 3 (class codes).
  {
    StreamEncoder enc(0x00000044, 8);
    auto std_ctx = enc.encode_context(1e9, 1e6, 2e6, 0.0, 1700000000u, 0);
    auto sc_ctx = enc.encode_context_sample_count(1e9, 1e6, 2e6, 0.0, 1700000000u, 0);
    CHECK(std_ctx.size() == sc_ctx.size());
    for (size_t off = 0; off < std_ctx.size(); ++off) {
      if (std_ctx[off] != sc_ctx[off]) {
        bool allowed = off < 4 || (off >= 12 && off < 16);
        CHECK(allowed);
      }
    }
  }
  // Regression: a 1.1 standard-flow context still decodes unchanged.
  {
    StreamEncoder enc(0x00000045, 16);
    auto bytes = enc.encode_context(1e9, 1e6, 2e6, 0.0, 1700000000u, 0);
    Decoder dec;
    DifiPacket p = dec.decode(bytes);
    CHECK(p.kind == PacketKind::SignalContext);
    CHECK(p.context->header.packet_class == constants::packet_class::SIGNAL_CONTEXT);
    CHECK(p.context->header.tsf == constants::tsf::REAL_TIME_PS);
  }
  // A type-0x4 context with an unknown class is rejected.
  {
    StreamEncoder enc(1, 16);
    auto bytes = enc.encode_context(1e9, 1e6, 2e6, 0.0, 1, 0);
    bytes[14] = 0x00;
    bytes[15] = 0x77;
    CHECK(throws([&] { SignalContextPacket::unpack(bytes); }));
  }
}

// DIFI 1.2 Timing Flow Control — spec difi-1.2-dataplane task 3. Mirrors
// rust/src/flowcontrol.rs tests; the two implementations MUST agree byte-for-byte.
static TimingFlowControlPacket make_fc(FlowControlKind kind) {
  TimingFlowControlPacket p{};
  p.header = make_header(constants::packet_type::COMMAND,
                         constants::information_class::DATA_PLANE_FLOW_CONTROL,
                         flow_control_packet_class(kind), 3);
  p.header.tsm = constants::tsm::DATA;
  p.header.tsf = flow_control_tsf(kind);
  p.header.stream_id = 0xABCD1234;
  p.kind = kind;
  p.cam.controllee_id_present = true;
  p.cam.controller_id_present = true;
  p.cam.action_mode = command::action_mode::EXECUTE;
  p.message_id = 7;
  p.controllee_id = 0x11112222;
  p.controller_id = 0x33334444;
  p.reference_point = constants::command::REF_POINT;
  p.sample_rate_hz = 30720000.0;
  p.timestamp_adjust_fs = -5000;
  p.buffer.size_bytes = 1u << 22;
  p.buffer.near_full = true;
  p.buffer.level = 0x00ABC000;
  return p;
}

static void test_flow_control() {
  // Sample-count round-trip.
  TimingFlowControlPacket sc = make_fc(FlowControlKind::SampleCount);
  auto b = sc.pack();
  CHECK(b.size() == 84);
  TimingFlowControlPacket back = TimingFlowControlPacket::unpack(b);
  CHECK(back.kind == FlowControlKind::SampleCount);
  CHECK(back.header.packet_class == constants::packet_class::SAMPLE_COUNT_FLOW_CONTROL);
  CHECK(back.header.tsf == constants::tsf::SAMPLE_COUNT);
  CHECK(back.message_id == 7);
  CHECK(back.controllee_id == 0x11112222);
  CHECK(back.controller_id == 0x33334444);
  CHECK(std::abs(back.sample_rate_hz - 30720000.0) < 1e-6);
  CHECK(back.timestamp_adjust_fs == -5000);
  CHECK(back == sc);
  // Real-time round-trip.
  TimingFlowControlPacket rt = make_fc(FlowControlKind::RealTime);
  TimingFlowControlPacket rtb = TimingFlowControlPacket::unpack(rt.pack());
  CHECK(rtb.kind == FlowControlKind::RealTime);
  CHECK(rtb.header.packet_class == constants::packet_class::REAL_TIME_FLOW_CONTROL);
  CHECK(rtb.header.tsf == constants::tsf::REAL_TIME_PS);
  // Buffer status flags survive.
  TimingFlowControlPacket fp = make_fc(FlowControlKind::RealTime);
  fp.buffer = command::BufferState{};
  fp.buffer.size_bytes = 9999;
  fp.buffer.overflow = true;
  fp.buffer.underflow = true;
  fp.buffer.near_empty = true;
  fp.buffer.level = 0x7F000000;
  TimingFlowControlPacket fpb = TimingFlowControlPacket::unpack(fp.pack());
  CHECK(fpb.buffer.overflow);
  CHECK(fpb.buffer.underflow);
  CHECK(!fpb.buffer.near_full);
  CHECK(fpb.buffer.near_empty);
  CHECK(fpb.buffer.level == 0x7F000000);
  // Fail-closed on wrong class / size.
  auto bad = sc.pack();
  bad[14] = 0x00;
  bad[15] = 0x99;
  CHECK(throws([&] { TimingFlowControlPacket::unpack(bad); }));
  std::vector<uint8_t> short_buf(sc.pack().begin(), sc.pack().begin() + 80);
  CHECK(throws([&] { TimingFlowControlPacket::unpack(short_buf); }));
  // The Decoder routes a flow-control packet with NO prior Signal Context for its stream
  // (the bit-depth gate must not block it).
  Decoder dec;
  DifiPacket dp = dec.decode(sc.pack());
  CHECK(dp.kind == PacketKind::TimingFlowControl);
  CHECK(dp.flow_control.has_value());
  CHECK(dp.flow_control->message_id == 7);
}

// DIFI 1.2 constants — spec difi-1.2-dataplane task 1. Mirrors
// rust/tests/constants_1_2.rs; the two implementations MUST agree.
static void test_constants_1_2() {
  namespace c = difi::constants;
  // DIFI 1.1 values unchanged.
  CHECK(c::VITA_OUI == 0x006A621Eu);
  CHECK(c::HEADER_WORDS == 7);
  CHECK(c::packet_type::SIGNAL_DATA == 0x1);
  CHECK(c::packet_type::CONTEXT == 0x4);
  CHECK(c::version_packet_type::VERSION_CONTEXT == 0x5);
  CHECK(c::information_class::STANDARD_FLOW == 0x0000);
  CHECK(c::information_class::VERSION_FLOW == 0x0001);
  CHECK(c::packet_class::SIGNAL_DATA == 0x0000);
  CHECK(c::packet_class::SIGNAL_CONTEXT == 0x0001);
  CHECK(c::packet_class::VERSION_CONTEXT == 0x0004);
  CHECK(c::context::WORDS == 27);
  CHECK(c::version::WORDS == 11);
  // DIFI 1.2 additions.
  CHECK(c::packet_type::COMMAND == 0x6);
  CHECK(c::information_class::DATA_PLANE_FLOW_CONTROL == 0x0002);
  CHECK(c::information_class::DATA_PLANE_FLOW_CONTROL_RT == 0x0003);
  CHECK(c::information_class::BASIC_DATA_PLANE_2 == 0x0004);
  CHECK(c::packet_class::SAMPLE_COUNT_SIGNAL_DATA == 0x0002);
  CHECK(c::packet_class::SAMPLE_COUNT_SIGNAL_CONTEXT == 0x0003);
  CHECK(c::packet_class::SAMPLE_COUNT_FLOW_CONTROL == 0x0005);
  CHECK(c::packet_class::REAL_TIME_FLOW_CONTROL == 0x0006);
  CHECK(c::command::WORDS == 21);
  CHECK(c::command::REF_POINT == 0x00000064u);
  CHECK(c::buffer_status::OVERFLOW_BIT == 3);
  CHECK(c::buffer_status::NEAR_FULL_BIT == 2);
  CHECK(c::buffer_status::NEAR_EMPTY_BIT == 1);
  CHECK(c::buffer_status::UNDERFLOW_BIT == 0);
  // Namespaces distinct but consistent where meant to line up.
  CHECK(c::packet_class::VERSION_CONTEXT == c::information_class::BASIC_DATA_PLANE_2);
  CHECK(c::packet_type::SIGNAL_DATA != c::packet_type::CONTEXT);
  CHECK(c::packet_type::CONTEXT != c::packet_type::COMMAND);
}

int main() {
  test_constants_1_2();
  test_command_core();
  test_flow_control();
  test_sample_count();
  test_iq_round_trip();
  test_iq_rejections();
  test_signal_data_round_trip();
  test_context_round_trips();
  test_decoder_fail_closed();
  test_stream_sequencing();
  test_stateful_decode_learns_depth();
  test_mapper_endpoints();
  test_mapper_clamps();
  test_mapper_loss_measured();
  test_mapper_rejects_bad_depth();
  test_property_iq_round_trip();
  test_fuzz_decoder_never_crashes();
  test_segmented_encode_reassembles();
  test_tcp_dev_mode_exchange();

  std::printf("%d checks, %d failures\n", g_checks, g_failures);
  return g_failures == 0 ? 0 : 1;
}
