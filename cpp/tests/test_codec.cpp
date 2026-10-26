// Unit tests for the DIFI 1.1 C++ codec (mirrors rust/tests/codec.rs).
// Minimal dependency-free harness: each CHECK records pass/fail; main returns nonzero
// if any failed.

#include <cmath>
#include <cstdio>
#include <functional>
#include <string>
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

int main() {
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

  std::printf("%d checks, %d failures\n", g_checks, g_failures);
  return g_failures == 0 ? 0 : 1;
}
