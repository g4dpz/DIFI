// difi-tool — minimal CLI for cross-implementation interop (C++ side).
//
// Subcommands mirror the Rust tool exactly so the two implementations can exchange
// packets and be diffed byte-for-byte:
//   gen-golden <dir>   Write the canonical golden packets to <dir>.
//   dump <file>        Decode a raw packet file and print its fields as text.

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

  if (!write_file(dir + "/signal_context_16bit.difi.bin", ctx) ||
      !write_file(dir + "/version_context.difi.bin", ver) ||
      !write_file(dir + "/signal_data_16bit.difi.bin", data)) {
    std::fprintf(stderr, "error: failed to write golden packets to %s\n", dir.c_str());
    return 1;
  }
  std::printf("wrote 3 golden packets to %s\n", dir.c_str());
  return 0;
}

int dump(const std::string& file) {
  auto buf = read_file(file);
  difi::Decoder dec;
  dec.set_bit_depth(STREAM_ID, 16);
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
    if (argc >= 3 && std::string(argv[1]) == "map-check")
      return map_check(static_cast<uint8_t>(std::stoul(argv[2])));
    usage();
    return 1;
  } catch (const std::exception& e) {
    std::fprintf(stderr, "error: %s\n", e.what());
    return 1;
  }
}
