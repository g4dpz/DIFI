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

  std::printf("%d checks, %d failures\n", g_checks, g_failures);
  return g_failures == 0 ? 0 : 1;
}
