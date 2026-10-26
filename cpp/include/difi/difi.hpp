// difi/difi.hpp — reference DIFI 1.1 I/Q codec (independent C++ implementation).
//
// A strict, fail-closed encoder/decoder for the DIFI 1.1 profile of VITA 49.2
// (IEEE-ISTO Std 4900-2021). This is one of two independent implementations in the
// difi-interop toolkit (the other is Rust); both are validated against the same
// committed golden packets so neither is "correct by its own definition".
//
// The wire format MUST stay byte-identical to the Rust implementation. See
// docs/reference/DIFI-VITA49-PRIMER.md and tests/golden/MANIFEST.md.

#ifndef DIFI_DIFI_HPP
#define DIFI_DIFI_HPP

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <limits>
#include <map>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace difi {

// ---------------------------------------------------------------------------
// Constants (must match rust/src/constants.rs)
// ---------------------------------------------------------------------------
namespace constants {
constexpr uint32_t VITA_OUI = 0x006A621E;
constexpr uint16_t HEADER_WORDS = 7;
constexpr size_t HEADER_BYTES = static_cast<size_t>(HEADER_WORDS) * 4;
constexpr size_t MAX_PAYLOAD_OCTETS = 9000;

namespace packet_type {
constexpr uint8_t SIGNAL_DATA = 0x1;
constexpr uint8_t CONTEXT = 0x4;
}  // namespace packet_type

// Version Context uses VRT packet type 0x5 (distinct from Signal Context's 0x4).
namespace version_packet_type {
constexpr uint8_t VERSION_CONTEXT = 0x5;
}

namespace information_class {
constexpr uint16_t STANDARD_FLOW = 0x0000;
constexpr uint16_t VERSION_FLOW = 0x0001;
}  // namespace information_class

constexpr uint16_t INFORMATION_CLASS_CODE = information_class::STANDARD_FLOW;

namespace packet_class {
constexpr uint16_t SIGNAL_DATA = 0x0000;
constexpr uint16_t SIGNAL_CONTEXT = 0x0001;
constexpr uint16_t VERSION_CONTEXT = 0x0004;
}  // namespace packet_class

// Fixed values required by the DIFI 1.1 Signal Context packet.
namespace context {
constexpr uint16_t WORDS = 27;
constexpr uint32_t CIF0_CHANGED = 0xFBB98000;
constexpr uint32_t CIF0_NO_CHANGE = 0x7BB98000;
constexpr uint32_t REF_POINT = 0x00000064;
}  // namespace context

// Fixed values required by the DIFI 1.1 Version Context packet.
namespace version {
constexpr uint16_t WORDS = 11;
constexpr uint32_t CIF0_CHANGED = 0x80000002;
constexpr uint32_t CIF0_NO_CHANGE = 0x00000002;
constexpr uint32_t CIF1 = 0x0000000C;
constexpr uint32_t V49_SPEC_VERSION = 0x00000004;
}  // namespace version

namespace scale {
constexpr double RADIX_20 = 1048576.0;  // 2^20
constexpr double RADIX_7 = 128.0;       // 2^7
}  // namespace scale

namespace tsm {
constexpr uint8_t DATA = 0;
constexpr uint8_t CONTEXT = 1;
}  // namespace tsm

namespace tsi {
constexpr uint8_t NONE = 0b00;
constexpr uint8_t UTC = 0b01;
constexpr uint8_t GPS = 0b10;
constexpr uint8_t OTHER = 0b11;
}  // namespace tsi

namespace tsf {
constexpr uint8_t NONE = 0b00;
constexpr uint8_t SAMPLE_COUNT = 0b01;
constexpr uint8_t REAL_TIME_PS = 0b10;
constexpr uint8_t FREE_RUNNING = 0b11;
}  // namespace tsf

constexpr uint8_t MIN_BIT_DEPTH = 4;
constexpr uint8_t MAX_BIT_DEPTH = 16;
}  // namespace constants

// ---------------------------------------------------------------------------
// Errors — fail-closed, field-level (mirrors rust/src/error.rs)
// ---------------------------------------------------------------------------
class DifiError : public std::runtime_error {
 public:
  explicit DifiError(const std::string& msg) : std::runtime_error(msg) {}
};

// ---------------------------------------------------------------------------
// Big-endian helpers
// ---------------------------------------------------------------------------
namespace detail {

inline void put_u32(std::vector<uint8_t>& out, uint32_t v) {
  out.push_back(static_cast<uint8_t>(v >> 24));
  out.push_back(static_cast<uint8_t>(v >> 16));
  out.push_back(static_cast<uint8_t>(v >> 8));
  out.push_back(static_cast<uint8_t>(v));
}

inline void put_u64(std::vector<uint8_t>& out, uint64_t v) {
  for (int s = 56; s >= 0; s -= 8) out.push_back(static_cast<uint8_t>(v >> s));
}

inline void put_f64(std::vector<uint8_t>& out, double v) {
  uint64_t bits;
  std::memcpy(&bits, &v, sizeof(bits));
  put_u64(out, bits);
}

inline void put_f32(std::vector<uint8_t>& out, float v) {
  uint32_t bits;
  std::memcpy(&bits, &v, sizeof(bits));
  put_u32(out, bits);
}

inline uint32_t get_u32(const std::vector<uint8_t>& b, size_t off) {
  return (static_cast<uint32_t>(b[off]) << 24) |
         (static_cast<uint32_t>(b[off + 1]) << 16) |
         (static_cast<uint32_t>(b[off + 2]) << 8) |
         static_cast<uint32_t>(b[off + 3]);
}

inline uint64_t get_u64(const std::vector<uint8_t>& b, size_t off) {
  uint64_t v = 0;
  for (size_t i = 0; i < 8; ++i) v = (v << 8) | b[off + i];
  return v;
}

inline void put_i64(std::vector<uint8_t>& out, int64_t v) {
  put_u64(out, static_cast<uint64_t>(v));
}

inline void put_i16(std::vector<uint8_t>& out, int16_t v) {
  uint16_t u = static_cast<uint16_t>(v);
  out.push_back(static_cast<uint8_t>(u >> 8));
  out.push_back(static_cast<uint8_t>(u));
}

inline int64_t get_i64(const std::vector<uint8_t>& b, size_t off) {
  return static_cast<int64_t>(get_u64(b, off));
}

inline int16_t get_i16(const std::vector<uint8_t>& b, size_t off) {
  return static_cast<int16_t>((static_cast<uint16_t>(b[off]) << 8) | b[off + 1]);
}

inline double get_f64(const std::vector<uint8_t>& b, size_t off) {
  uint64_t bits = get_u64(b, off);
  double v;
  std::memcpy(&v, &bits, sizeof(v));
  return v;
}

inline float get_f32(const std::vector<uint8_t>& b, size_t off) {
  uint32_t bits = get_u32(b, off);
  float v;
  std::memcpy(&v, &bits, sizeof(v));
  return v;
}

}  // namespace detail

// ---------------------------------------------------------------------------
// I/Q sample packing (mirrors rust/src/iq.rs)
// ---------------------------------------------------------------------------
struct IqSample {
  int32_t i;
  int32_t q;
  bool operator==(const IqSample& o) const { return i == o.i && q == o.q; }
  bool operator!=(const IqSample& o) const { return !(*this == o); }
};

inline void check_bit_depth(uint8_t bit_depth) {
  if (bit_depth < constants::MIN_BIT_DEPTH || bit_depth > constants::MAX_BIT_DEPTH) {
    throw DifiError("unsupported bit depth " + std::to_string(bit_depth) +
                    " (must be 4..=16)");
  }
}

inline std::pair<int32_t, int32_t> sample_range(uint8_t bit_depth) {
  int32_t max = (static_cast<int32_t>(1) << (bit_depth - 1)) - 1;
  int32_t min = -(static_cast<int32_t>(1) << (bit_depth - 1));
  return {min, max};
}

inline size_t payload_word_count(size_t sample_count, uint8_t bit_depth) {
  check_bit_depth(bit_depth);
  size_t total_bits = sample_count * 2 * static_cast<size_t>(bit_depth);
  if (total_bits % 32 != 0) {
    throw DifiError("samples*2*bit_depth (bits) = " + std::to_string(total_bits) +
                    " is not 4-byte-word aligned");
  }
  return total_bits / 32;
}

inline std::vector<uint8_t> pack_samples(const std::vector<IqSample>& samples,
                                         uint8_t bit_depth) {
  check_bit_depth(bit_depth);
  auto range = sample_range(bit_depth);
  const int32_t min = range.first;
  const int32_t max = range.second;
  size_t words = payload_word_count(samples.size(), bit_depth);
  std::vector<uint8_t> out(words * 4, 0);

  size_t bit_pos = 0;
  uint32_t mask = (bit_depth >= 32) ? 0xFFFFFFFFu
                                    : ((1u << bit_depth) - 1u);

  auto write_component = [&](int32_t value) {
    if (value < min || value > max) {
      throw DifiError("sample " + std::to_string(value) + " does not fit in " +
                      std::to_string(bit_depth) + "-bit signed");
    }
    uint32_t bits = static_cast<uint32_t>(value) & mask;
    for (int k = static_cast<int>(bit_depth) - 1; k >= 0; --k) {
      uint32_t bit = (bits >> k) & 1u;
      if (bit) {
        size_t byte = bit_pos / 8;
        size_t shift = 7 - (bit_pos % 8);
        out[byte] |= static_cast<uint8_t>(bit << shift);
      }
      ++bit_pos;
    }
  };

  for (const auto& s : samples) {
    write_component(s.i);
    write_component(s.q);
  }
  return out;
}

inline std::vector<IqSample> unpack_samples(const std::vector<uint8_t>& buf,
                                            size_t offset, size_t sample_count,
                                            uint8_t bit_depth) {
  check_bit_depth(bit_depth);
  size_t words = payload_word_count(sample_count, bit_depth);
  size_t need = offset + words * 4;
  if (buf.size() < need) {
    throw DifiError("truncated at field 'iq payload': need " + std::to_string(need) +
                    " bytes, have " + std::to_string(buf.size()));
  }

  size_t bit_pos = offset * 8;
  uint32_t sign_bit = 1u << (bit_depth - 1);
  uint32_t full_mask = (1u << bit_depth) - 1u;

  auto read_component = [&]() -> int32_t {
    uint32_t bits = 0;
    for (uint8_t n = 0; n < bit_depth; ++n) {
      size_t byte = bit_pos / 8;
      size_t shift = 7 - (bit_pos % 8);
      uint32_t bit = (buf[byte] >> shift) & 1u;
      bits = (bits << 1) | bit;
      ++bit_pos;
    }
    if (bits & sign_bit) {
      return static_cast<int32_t>(bits | ~full_mask);
    }
    return static_cast<int32_t>(bits);
  };

  std::vector<IqSample> out;
  out.reserve(sample_count);
  for (size_t n = 0; n < sample_count; ++n) {
    int32_t i = read_component();
    int32_t q = read_component();
    out.push_back({i, q});
  }
  return out;
}

// ---------------------------------------------------------------------------
// Packet model (mirrors rust/src/packet.rs)
// ---------------------------------------------------------------------------
struct Header {
  uint8_t packet_type;
  bool class_id_present;
  uint8_t tsm;  // 0 for Signal Data, 1 for Context/Version
  uint8_t tsi;
  uint8_t tsf;
  uint8_t seq_num;  // mod-16
  uint32_t stream_id;
  uint16_t information_class;
  uint16_t packet_class;
  uint32_t integer_seconds;
  uint64_t fractional_seconds;

  void write_into(uint16_t packet_size_words, std::vector<uint8_t>& out) const {
    uint32_t word0 =
        ((static_cast<uint32_t>(packet_type) & 0xF) << 28) |
        ((class_id_present ? 1u : 0u) << 27) |
        ((static_cast<uint32_t>(tsm) & 0x1) << 24) |
        ((static_cast<uint32_t>(tsi) & 0x3) << 22) |
        ((static_cast<uint32_t>(tsf) & 0x3) << 20) |
        ((static_cast<uint32_t>(seq_num) & 0xF) << 16) |
        (static_cast<uint32_t>(packet_size_words) & 0xFFFF);
    detail::put_u32(out, word0);
    detail::put_u32(out, stream_id);
    detail::put_u32(out, constants::VITA_OUI & 0x00FFFFFF);
    detail::put_u32(out, (static_cast<uint32_t>(information_class) << 16) |
                             static_cast<uint32_t>(packet_class));
    detail::put_u32(out, integer_seconds);
    detail::put_u64(out, fractional_seconds);
  }

  static std::pair<Header, uint16_t> parse(const std::vector<uint8_t>& buf) {
    if (buf.size() < constants::HEADER_BYTES) {
      throw DifiError("truncated at field 'header': need " +
                      std::to_string(constants::HEADER_BYTES) + " bytes, have " +
                      std::to_string(buf.size()));
    }
    uint32_t word0 = detail::get_u32(buf, 0);
    Header h{};
    h.packet_type = static_cast<uint8_t>((word0 >> 28) & 0xF);
    h.class_id_present = ((word0 >> 27) & 1) == 1;
    h.tsm = static_cast<uint8_t>((word0 >> 24) & 0x1);
    h.tsi = static_cast<uint8_t>((word0 >> 22) & 0x3);
    h.tsf = static_cast<uint8_t>((word0 >> 20) & 0x3);
    h.seq_num = static_cast<uint8_t>((word0 >> 16) & 0xF);
    uint16_t packet_size = static_cast<uint16_t>(word0 & 0xFFFF);
    h.stream_id = detail::get_u32(buf, 4);
    uint32_t oui = detail::get_u32(buf, 8) & 0x00FFFFFF;
    if (oui != (constants::VITA_OUI & 0x00FFFFFF)) {
      char msg[96];
      std::snprintf(msg, sizeof(msg),
                    "bad OUI: expected 0x%06X, got 0x%06X (not a DIFI packet)",
                    constants::VITA_OUI & 0x00FFFFFF, oui);
      throw DifiError(msg);
    }
    uint32_t word3 = detail::get_u32(buf, 12);
    h.information_class = static_cast<uint16_t>(word3 >> 16);
    h.packet_class = static_cast<uint16_t>(word3 & 0xFFFF);
    h.integer_seconds = detail::get_u32(buf, 16);
    h.fractional_seconds = detail::get_u64(buf, 20);
    return {h, packet_size};
  }

  bool operator==(const Header& o) const {
    return packet_type == o.packet_type && class_id_present == o.class_id_present &&
           tsm == o.tsm && tsi == o.tsi && tsf == o.tsf && seq_num == o.seq_num &&
           stream_id == o.stream_id && information_class == o.information_class &&
           packet_class == o.packet_class && integer_seconds == o.integer_seconds &&
           fractional_seconds == o.fractional_seconds;
  }
};

inline void validate_size(uint16_t declared_words, size_t actual_bytes) {
  if (actual_bytes % 4 != 0) {
    throw DifiError("'packet length' = " + std::to_string(actual_bytes) +
                    " is not 4-byte-word aligned");
  }
  if (static_cast<size_t>(declared_words) * 4 != actual_bytes) {
    throw DifiError("packet-size mismatch: declared " + std::to_string(declared_words) +
                    " words (" + std::to_string(declared_words * 4) +
                    " bytes), buffer is " + std::to_string(actual_bytes) + " bytes");
  }
}

struct DataFormat {
  uint8_t bit_depth;
  bool operator==(const DataFormat& o) const { return bit_depth == o.bit_depth; }
};

struct SignalDataPacket {
  Header header;
  uint8_t bit_depth;
  std::vector<IqSample> samples;

  std::vector<uint8_t> pack() const {
    size_t payload_words = payload_word_count(samples.size(), bit_depth);
    size_t total_words = constants::HEADER_WORDS + payload_words;
    if (total_words > 0xFFFF) {
      throw DifiError("invalid 'packet_size': " + std::to_string(total_words) +
                      " words exceeds 16-bit field");
    }
    size_t payload_octets = total_words * 4;
    if (payload_octets > constants::MAX_PAYLOAD_OCTETS) {
      throw DifiError("payload " + std::to_string(payload_octets) +
                      " octets exceeds DIFI maximum " +
                      std::to_string(constants::MAX_PAYLOAD_OCTETS));
    }
    auto payload = pack_samples(samples, bit_depth);
    std::vector<uint8_t> out;
    out.reserve(total_words * 4);
    header.write_into(static_cast<uint16_t>(total_words), out);
    out.insert(out.end(), payload.begin(), payload.end());
    return out;
  }

  static SignalDataPacket unpack(const std::vector<uint8_t>& buf, uint8_t bit_depth) {
    auto [header, declared_words] = Header::parse(buf);
    validate_size(declared_words, buf.size());
    if (header.packet_type != constants::packet_type::SIGNAL_DATA) {
      throw DifiError("unsupported VRT packet type " +
                      std::to_string(header.packet_type));
    }
    size_t payload_bytes = static_cast<size_t>(declared_words) * 4 - constants::HEADER_BYTES;
    size_t total_bits = (payload_bytes / 4) * 32;
    if (total_bits % (2u * bit_depth) != 0) {
      throw DifiError("payload bits / (2*bit_depth) not aligned");
    }
    size_t sample_count = total_bits / (2u * bit_depth);
    auto samples = unpack_samples(buf, constants::HEADER_BYTES, sample_count, bit_depth);
    return SignalDataPacket{header, bit_depth, samples};
  }
};

namespace detail {
inline int64_t to_fixed_i64(double v, double radix) {
  return static_cast<int64_t>(std::llround(v * radix));
}
inline double from_fixed_i64(int64_t raw, double radix) {
  return static_cast<double>(raw) / radix;
}
inline int16_t to_fixed_i16(double v, double radix) {
  return static_cast<int16_t>(std::lround(v * radix));
}
inline double from_fixed_i16(int16_t raw, double radix) {
  return static_cast<double>(raw) / radix;
}

// dataPacketFormat: bit depth carried in item_packing_field_size and data_item_size
// as (bits - 1). Returns {word0, word1}.
inline std::pair<uint32_t, uint32_t> encode_data_packet_format(uint8_t bit_depth) {
  uint32_t packing_method = 1;   // link_efficient
  uint32_t real_complex = 1;     // complex_cartesian
  uint32_t data_item_format = 0; // signed_fixed_point
  uint32_t ipfs = (static_cast<uint32_t>(bit_depth) - 1) & 0x3F;
  uint32_t dis = (static_cast<uint32_t>(bit_depth) - 1) & 0x3F;
  uint32_t word0 = (packing_method << 31) | (real_complex << 29) |
                   (data_item_format << 24) | (ipfs << 6) | dis;
  return {word0, 0u};
}

inline uint8_t decode_data_packet_format(uint32_t word0) {
  uint8_t dis = static_cast<uint8_t>(word0 & 0x3F);
  uint8_t bit_depth = dis + 1;
  check_bit_depth(bit_depth);
  return bit_depth;
}
}  // namespace detail

struct SignalContextPacket {
  Header header;
  bool context_changed;
  double rf_reference_hz;
  double if_reference_hz;
  double bandwidth_hz;
  double sample_rate_hz;
  double gain_db;
  DataFormat data_format;

  std::vector<uint8_t> pack() const {
    check_bit_depth(data_format.bit_depth);
    std::vector<uint8_t> out;
    out.reserve(constants::context::WORDS * 4);
    header.write_into(constants::context::WORDS, out);
    detail::put_u32(out, context_changed ? constants::context::CIF0_CHANGED
                                         : constants::context::CIF0_NO_CHANGE);
    detail::put_u32(out, constants::context::REF_POINT);
    detail::put_u64(out, static_cast<uint64_t>(
                             detail::to_fixed_i64(bandwidth_hz, constants::scale::RADIX_20)));
    detail::put_i64(out, detail::to_fixed_i64(if_reference_hz, constants::scale::RADIX_20));
    detail::put_i64(out, detail::to_fixed_i64(rf_reference_hz, constants::scale::RADIX_20));
    detail::put_i64(out, 0);  // ifBandOffset
    detail::put_i16(out, 0);  // refLevel1
    detail::put_i16(out, 0);  // refLevel2
    detail::put_i16(out, detail::to_fixed_i16(gain_db, constants::scale::RADIX_7));
    detail::put_i16(out, 0);  // stage2GainAtten
    detail::put_u64(out, static_cast<uint64_t>(
                             detail::to_fixed_i64(sample_rate_hz, constants::scale::RADIX_20)));
    detail::put_i64(out, 0);  // timeStampAdj
    detail::put_u32(out, 0);  // timeStampCal
    detail::put_u32(out, 0);  // stateEventInd
    auto df = detail::encode_data_packet_format(data_format.bit_depth);
    detail::put_u32(out, df.first);
    detail::put_u32(out, df.second);
    return out;
  }

  static SignalContextPacket unpack(const std::vector<uint8_t>& buf) {
    auto [header, declared_words] = Header::parse(buf);
    validate_size(declared_words, buf.size());
    if (header.packet_type != constants::packet_type::CONTEXT)
      throw DifiError("unsupported VRT packet type " + std::to_string(header.packet_type));
    if (header.packet_class != constants::packet_class::SIGNAL_CONTEXT)
      throw DifiError("unrecognised packet class code");
    size_t need = static_cast<size_t>(constants::context::WORDS) * 4;
    if (buf.size() < need) throw DifiError("truncated signal-context payload");
    SignalContextPacket p{};
    p.header = header;
    uint32_t cif0 = detail::get_u32(buf, 28);
    p.context_changed = (cif0 == constants::context::CIF0_CHANGED);
    p.bandwidth_hz = detail::from_fixed_i64(static_cast<int64_t>(detail::get_u64(buf, 36)),
                                            constants::scale::RADIX_20);
    p.if_reference_hz = detail::from_fixed_i64(detail::get_i64(buf, 44), constants::scale::RADIX_20);
    p.rf_reference_hz = detail::from_fixed_i64(detail::get_i64(buf, 52), constants::scale::RADIX_20);
    p.gain_db = detail::from_fixed_i16(detail::get_i16(buf, 72), constants::scale::RADIX_7);
    p.sample_rate_hz = detail::from_fixed_i64(static_cast<int64_t>(detail::get_u64(buf, 76)),
                                              constants::scale::RADIX_20);
    uint8_t bit_depth = detail::decode_data_packet_format(detail::get_u32(buf, 100));
    p.data_format = DataFormat{bit_depth};
    return p;
  }
};

struct VersionContextPacket {
  Header header;
  bool context_changed;
  uint16_t year;  // full year, e.g. 2021
  uint16_t day;   // 1-366

  std::vector<uint8_t> pack() const {
    std::vector<uint8_t> out;
    out.reserve(constants::version::WORDS * 4);
    header.write_into(constants::version::WORDS, out);
    detail::put_u32(out, context_changed ? constants::version::CIF0_CHANGED
                                         : constants::version::CIF0_NO_CHANGE);
    detail::put_u32(out, constants::version::CIF1);
    detail::put_u32(out, constants::version::V49_SPEC_VERSION);
    int32_t yr = static_cast<int32_t>(year) - 2000;
    if (yr < 0) yr = 0;
    if (yr > 127) yr = 127;
    uint32_t version_info = (static_cast<uint32_t>(yr) << 25) |
                            ((static_cast<uint32_t>(day) & 0x1FF) << 16) |
                            (1u << 10);  // revision = 1, type = 0, icdVersion = 0
    detail::put_u32(out, version_info);
    return out;
  }

  static VersionContextPacket unpack(const std::vector<uint8_t>& buf) {
    auto [header, declared_words] = Header::parse(buf);
    validate_size(declared_words, buf.size());
    if (header.packet_type != constants::version_packet_type::VERSION_CONTEXT)
      throw DifiError("unsupported VRT packet type " + std::to_string(header.packet_type));
    if (header.packet_class != constants::packet_class::VERSION_CONTEXT)
      throw DifiError("unrecognised packet class code");
    size_t need = static_cast<size_t>(constants::version::WORDS) * 4;
    if (buf.size() < need) throw DifiError("truncated version-context payload");
    VersionContextPacket p{};
    p.header = header;
    uint32_t cif0 = detail::get_u32(buf, 28);
    p.context_changed = (cif0 == constants::version::CIF0_CHANGED);
    uint32_t vi = detail::get_u32(buf, 40);
    p.year = static_cast<uint16_t>(((vi >> 25) & 0x7F) + 2000);
    p.day = static_cast<uint16_t>((vi >> 16) & 0x1FF);
    return p;
  }

  bool operator==(const VersionContextPacket& o) const {
    return header == o.header && context_changed == o.context_changed &&
           year == o.year && day == o.day;
  }
};

// ---------------------------------------------------------------------------
// Encoder (mirrors rust/src/encoder.rs)
// ---------------------------------------------------------------------------
class StreamEncoder {
 public:
  StreamEncoder(uint32_t stream_id, uint8_t bit_depth)
      : stream_id_(stream_id), bit_depth_(bit_depth), seq_(0) {}

  uint32_t stream_id() const { return stream_id_; }
  uint8_t bit_depth() const { return bit_depth_; }
  uint8_t next_seq() const { return seq_; }

  std::vector<uint8_t> encode_data(const std::vector<IqSample>& samples,
                                   uint32_t integer_seconds, uint64_t fractional_ps) {
    Header h = header(constants::packet_type::SIGNAL_DATA, constants::tsm::DATA,
                      constants::information_class::STANDARD_FLOW,
                      constants::packet_class::SIGNAL_DATA, seq_, integer_seconds,
                      fractional_ps);
    SignalDataPacket pkt{h, bit_depth_, samples};
    auto bytes = pkt.pack();
    seq_ = (seq_ + 1) & 0xF;
    return bytes;
  }

  // Largest word-aligned complex-sample count that fits one Signal Data packet within
  // the DIFI max payload (Req 2.5). Mirrors rust StreamEncoder::max_samples_per_packet.
  size_t max_samples_per_packet() const {
    size_t bd = bit_depth_;
    auto gcd = [](size_t a, size_t b) {
      while (b) { size_t t = a % b; a = b; b = t; }
      return a;
    };
    size_t step = (16 * bd / gcd(16, bd)) / bd;  // lcm(16,bd)/bd: min aligned block
    size_t payload_bits = (constants::MAX_PAYLOAD_OCTETS - constants::HEADER_BYTES) * 8;
    size_t max_unaligned = payload_bits / (2 * bd);
    return (max_unaligned / step) * step;
  }

  // Encode a (possibly large) buffer as a sequence of Signal Data packets, segmenting at
  // the max-payload boundary (Req 2.5). Running mod-16 sequence; one buffer per packet.
  std::vector<std::vector<uint8_t>> encode_data_segmented(
      const std::vector<IqSample>& samples, uint32_t integer_seconds,
      uint64_t fractional_ps) {
    size_t chunk = max_samples_per_packet();
    if (chunk == 0) throw DifiError("no whole-word sample chunk fits the max payload");
    std::vector<std::vector<uint8_t>> out;
    for (size_t off = 0; off < samples.size(); off += chunk) {
      size_t end = std::min(off + chunk, samples.size());
      std::vector<IqSample> part(samples.begin() + off, samples.begin() + end);
      out.push_back(encode_data(part, integer_seconds, fractional_ps));
    }
    return out;
  }

  std::vector<uint8_t> encode_context(double rf_reference_hz, double bandwidth_hz,
                                      double sample_rate_hz, double gain_db,
                                      uint32_t integer_seconds,
                                      uint64_t fractional_ps) const {
    Header h = header(constants::packet_type::CONTEXT, constants::tsm::CONTEXT,
                      constants::information_class::STANDARD_FLOW,
                      constants::packet_class::SIGNAL_CONTEXT, 0, integer_seconds,
                      fractional_ps);
    SignalContextPacket pkt{h,    true,          rf_reference_hz, 0.0,
                            bandwidth_hz, sample_rate_hz, gain_db,
                            DataFormat{bit_depth_}};
    return pkt.pack();
  }

  std::vector<uint8_t> encode_version(uint16_t year, uint16_t day,
                                      uint32_t integer_seconds,
                                      uint64_t fractional_ps) const {
    Header h = header(constants::version_packet_type::VERSION_CONTEXT,
                      constants::tsm::CONTEXT,
                      constants::information_class::VERSION_FLOW,
                      constants::packet_class::VERSION_CONTEXT, 0, integer_seconds,
                      fractional_ps);
    VersionContextPacket pkt{h, true, year, day};
    return pkt.pack();
  }

 private:
  Header header(uint8_t packet_type, uint8_t tsm_bit, uint16_t information_class,
                uint16_t packet_class, uint8_t seq, uint32_t int_s,
                uint64_t frac_ps) const {
    Header h{};
    h.packet_type = packet_type;
    h.class_id_present = true;
    h.tsm = tsm_bit;
    h.tsi = constants::tsi::UTC;
    h.tsf = constants::tsf::REAL_TIME_PS;
    h.seq_num = seq & 0xF;
    h.stream_id = stream_id_;
    h.information_class = information_class;
    h.packet_class = packet_class;
    h.integer_seconds = int_s;
    h.fractional_seconds = frac_ps;
    return h;
  }

  uint32_t stream_id_;
  uint8_t bit_depth_;
  uint8_t seq_;
};

// ---------------------------------------------------------------------------
// Decoder (mirrors rust/src/decoder.rs)
// ---------------------------------------------------------------------------
enum class PacketKind { SignalData, SignalContext, VersionContext };

struct DifiPacket {
  PacketKind kind;
  std::optional<SignalDataPacket> data;
  std::optional<SignalContextPacket> context;
  std::optional<VersionContextPacket> version;
};

class Decoder {
 public:
  void set_bit_depth(uint32_t stream_id, uint8_t bit_depth) {
    bit_depth_by_stream_[stream_id] = bit_depth;
  }

  DifiPacket decode(const std::vector<uint8_t>& buf) {
    if (buf.size() < constants::HEADER_BYTES) {
      throw DifiError("truncated at field 'header': need " +
                      std::to_string(constants::HEADER_BYTES) + " bytes, have " +
                      std::to_string(buf.size()));
    }
    uint32_t word0 = detail::get_u32(buf, 0);
    uint8_t packet_type = static_cast<uint8_t>((word0 >> 28) & 0xF);
    uint32_t oui = detail::get_u32(buf, 8) & 0x00FFFFFF;
    if (oui != (constants::VITA_OUI & 0x00FFFFFF)) {
      char msg[96];
      std::snprintf(msg, sizeof(msg),
                    "bad OUI: expected 0x%06X, got 0x%06X (not a DIFI packet)",
                    constants::VITA_OUI & 0x00FFFFFF, oui);
      throw DifiError(msg);
    }
    uint32_t word3 = detail::get_u32(buf, 12);
    uint16_t packet_class = static_cast<uint16_t>(word3 & 0xFFFF);
    uint32_t stream_id = detail::get_u32(buf, 4);

    DifiPacket result{};
    if (packet_type == constants::packet_type::CONTEXT &&
        packet_class == constants::packet_class::SIGNAL_CONTEXT) {
      auto ctx = SignalContextPacket::unpack(buf);
      bit_depth_by_stream_[ctx.header.stream_id] = ctx.data_format.bit_depth;
      result.kind = PacketKind::SignalContext;
      result.context = ctx;
      return result;
    }
    if (packet_type == constants::version_packet_type::VERSION_CONTEXT) {
      result.kind = PacketKind::VersionContext;
      result.version = VersionContextPacket::unpack(buf);
      return result;
    }
    if (packet_type == constants::packet_type::SIGNAL_DATA) {
      auto it = bit_depth_by_stream_.find(stream_id);
      if (it == bit_depth_by_stream_.end()) {
        char msg[128];
        std::snprintf(msg, sizeof(msg),
                      "invalid 'bit_depth': no Signal Context seen for stream "
                      "0x%08X; cannot decode data",
                      stream_id);
        throw DifiError(msg);
      }
      result.kind = PacketKind::SignalData;
      result.data = SignalDataPacket::unpack(buf, it->second);
      return result;
    }
    throw DifiError("unsupported VRT packet type " + std::to_string(packet_type));
  }

 private:
  std::map<uint32_t, uint8_t> bit_depth_by_stream_;
};

// ---------------------------------------------------------------------------
// SHA-256 (FIPS 180-4), dependency-free (mirrors rust/src/sha256.rs)
// ---------------------------------------------------------------------------
namespace detail {

inline std::string sha256_hex(const std::vector<uint8_t>& data) {
  static const uint32_t K[64] = {
      0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
      0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
      0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
      0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
      0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
      0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
      0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
      0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
      0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
      0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
      0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2};
  uint32_t h[8] = {0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                   0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19};

  std::vector<uint8_t> msg = data;
  uint64_t bit_len = static_cast<uint64_t>(data.size()) * 8;
  msg.push_back(0x80);
  while (msg.size() % 64 != 56) msg.push_back(0);
  for (int s = 56; s >= 0; s -= 8) msg.push_back(static_cast<uint8_t>(bit_len >> s));

  auto rotr = [](uint32_t x, uint32_t n) { return (x >> n) | (x << (32 - n)); };
  for (size_t base = 0; base < msg.size(); base += 64) {
    uint32_t w[64];
    for (int i = 0; i < 16; ++i)
      w[i] = (uint32_t(msg[base + i * 4]) << 24) | (uint32_t(msg[base + i * 4 + 1]) << 16) |
             (uint32_t(msg[base + i * 4 + 2]) << 8) | uint32_t(msg[base + i * 4 + 3]);
    for (int i = 16; i < 64; ++i) {
      uint32_t s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >> 3);
      uint32_t s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >> 10);
      w[i] = w[i - 16] + s0 + w[i - 7] + s1;
    }
    uint32_t a = h[0], b = h[1], c = h[2], d = h[3], e = h[4], f = h[5], g = h[6],
             hh = h[7];
    for (int i = 0; i < 64; ++i) {
      uint32_t s1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      uint32_t ch = (e & f) ^ ((~e) & g);
      uint32_t t1 = hh + s1 + ch + K[i] + w[i];
      uint32_t s0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      uint32_t maj = (a & b) ^ (a & c) ^ (b & c);
      uint32_t t2 = s0 + maj;
      hh = g; g = f; f = e; e = d + t1; d = c; c = b; b = a; a = t1 + t2;
    }
    h[0] += a; h[1] += b; h[2] += c; h[3] += d;
    h[4] += e; h[5] += f; h[6] += g; h[7] += hh;
  }

  char out[65];
  for (int i = 0; i < 8; ++i) std::snprintf(out + i * 8, 9, "%08x", h[i]);
  return std::string(out, 64);
}

}  // namespace detail

// ---------------------------------------------------------------------------
// Reference-signal generator (mirrors rust/src/signal.rs)
//
// Reproduces a float32 complex baseband signal identically to the Rust generator, so
// the same bytes can be regenerated on demand and exchanged over DIFI (task 12).
// ---------------------------------------------------------------------------
struct SignalParams {
  size_t num_samples = 4096;
  double sample_rate_hz = 1.25e6;
  double tone1_hz = 50000.0;
  double tone2_hz = -120000.0;
  float peak = 0.9f;
};

inline std::vector<std::pair<float, float>> generate_signal(const SignalParams& p) {
  const double TAU = 2.0 * M_PI;
  double w1 = TAU * p.tone1_hz / p.sample_rate_hz;
  double w2 = TAU * p.tone2_hz / p.sample_rate_hz;

  std::vector<std::pair<double, double>> raw;
  raw.reserve(p.num_samples);
  double max_mag = 0.0;
  for (size_t k = 0; k < p.num_samples; ++k) {
    double t = static_cast<double>(k);
    double env = 0.75 + 0.25 * std::cos(TAU * static_cast<double>(k) /
                                        static_cast<double>(p.num_samples));
    double re = env * (std::cos(w1 * t) + 0.6 * std::cos(w2 * t));
    double im = env * (std::sin(w1 * t) + 0.6 * std::sin(w2 * t));
    double mag = std::sqrt(re * re + im * im);
    if (mag > max_mag) max_mag = mag;
    raw.push_back({re, im});
  }
  double scale = (max_mag > 0.0) ? (static_cast<double>(p.peak) / max_mag) : 0.0;
  std::vector<std::pair<float, float>> out;
  out.reserve(p.num_samples);
  for (auto& s : raw)
    out.push_back({static_cast<float>(s.first * scale),
                   static_cast<float>(s.second * scale)});
  return out;
}

// Encode float32 complex I/Q to cf32 bytes: interleaved I,Q little-endian float32
// (mirrors rust/src/manifest.rs encode_cf32).
inline std::vector<uint8_t> encode_cf32(const std::vector<std::pair<float, float>>& iq) {
  std::vector<uint8_t> out;
  out.reserve(iq.size() * 8);
  auto put_le_f32 = [&out](float v) {
    uint32_t bits;
    std::memcpy(&bits, &v, 4);
    out.push_back(static_cast<uint8_t>(bits));
    out.push_back(static_cast<uint8_t>(bits >> 8));
    out.push_back(static_cast<uint8_t>(bits >> 16));
    out.push_back(static_cast<uint8_t>(bits >> 24));
  };
  for (auto& s : iq) {
    put_le_f32(s.first);
    put_le_f32(s.second);
  }
  return out;
}

inline std::string sha256_hex(const std::vector<uint8_t>& data) {
  return detail::sha256_hex(data);
}

// ---------------------------------------------------------------------------
// Sample Mapper — float ↔ DIFI integer (mirrors rust/src/mapper.rs)
//
// Convention (shared, documented — DQ-2): a float sample is a normalised amplitude
// where `full_scale` maps to the positive full-scale code FS = 2^(n-1) - 1, applied
// symmetrically to I and Q.
//   float -> int: code = round(clamp(x / full_scale, -1, 1) * FS)  (round half away
//                 from zero; over-range saturates, never wraps)
//   int -> float: x = code / FS * full_scale
// Round trip is NOT lossless; measure_implementation_loss reports the actual error.
// ---------------------------------------------------------------------------
struct ScalingConvention {
  uint8_t bit_depth;
  double full_scale;

  ScalingConvention(uint8_t depth, double fs) : bit_depth(depth), full_scale(fs) {
    check_bit_depth(depth);
  }

  int32_t fs_code() const { return sample_range(bit_depth).second; }

  int32_t float_to_code(float x) const {
    auto range = sample_range(bit_depth);
    const int32_t mn = range.first;
    const int32_t mx = range.second;
    const double fs = static_cast<double>(mx);
    double norm = static_cast<double>(x) / full_scale;
    if (norm > 1.0) norm = 1.0;
    if (norm < -1.0) norm = -1.0;
    double scaled = norm * fs;
    double rounded = (scaled >= 0.0) ? std::floor(scaled + 0.5) : std::ceil(scaled - 0.5);
    int32_t code = static_cast<int32_t>(rounded);
    if (code < mn) code = mn;
    if (code > mx) code = mx;
    return code;
  }

  float code_to_float(int32_t code) const {
    const double fs = static_cast<double>(sample_range(bit_depth).second);
    return static_cast<float>((static_cast<double>(code) / fs) * full_scale);
  }
};

inline std::vector<IqSample> float_to_difi(const std::vector<std::pair<float, float>>& iq,
                                           const ScalingConvention& conv) {
  std::vector<IqSample> out;
  out.reserve(iq.size());
  for (const auto& s : iq)
    out.push_back({conv.float_to_code(s.first), conv.float_to_code(s.second)});
  return out;
}

inline std::vector<std::pair<float, float>> difi_to_float(
    const std::vector<IqSample>& samples, const ScalingConvention& conv) {
  std::vector<std::pair<float, float>> out;
  out.reserve(samples.size());
  for (const auto& s : samples)
    out.push_back({conv.code_to_float(s.i), conv.code_to_float(s.q)});
  return out;
}

struct LossReport {
  size_t sample_count;
  double max_abs_error;
  double rms_error;
  double snr_db;
};

inline LossReport measure_implementation_loss(
    const std::vector<std::pair<float, float>>& iq, const ScalingConvention& conv) {
  auto codes = float_to_difi(iq, conv);
  auto recovered = difi_to_float(codes, conv);

  double sum_sq_err = 0.0, sum_sq_sig = 0.0, max_abs = 0.0;
  size_t n_components = iq.size() * 2;
  for (size_t k = 0; k < iq.size(); ++k) {
    const double origs[2] = {iq[k].first, iq[k].second};
    const double recs[2] = {recovered[k].first, recovered[k].second};
    for (int c = 0; c < 2; ++c) {
      double e = recs[c] - origs[c];
      sum_sq_err += e * e;
      sum_sq_sig += origs[c] * origs[c];
      if (std::fabs(e) > max_abs) max_abs = std::fabs(e);
    }
  }
  double rms = (n_components > 0) ? std::sqrt(sum_sq_err / n_components) : 0.0;
  double snr = (sum_sq_err == 0.0) ? std::numeric_limits<double>::infinity()
                                   : 10.0 * std::log10(sum_sq_sig / sum_sq_err);
  return LossReport{iq.size(), max_abs, rms, snr};
}

// ---------------------------------------------------------------------------
// UDP transport (mirrors rust/src/transport.rs; POSIX sockets)
// ---------------------------------------------------------------------------
}  // namespace difi

#include <arpa/inet.h>
#include <netdb.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <unistd.h>

#include <cerrno>

namespace difi {

// Read the Stream ID (word 1) from a raw DIFI packet without full decoding.
// Returns false if the buffer is too short.
inline bool peek_stream_id(const std::vector<uint8_t>& buf, uint32_t& out) {
  if (buf.size() < constants::HEADER_BYTES) return false;
  out = detail::get_u32(buf, 4);
  return true;
}

namespace detail {

// Resolve "host:port" into a sockaddr_in. IPv4 only, which is all the interop
// harness needs; keeps the transport dependency-free and portable.
inline bool resolve_v4(const std::string& host, uint16_t port, sockaddr_in& out) {
  std::memset(&out, 0, sizeof(out));
  out.sin_family = AF_INET;
  out.sin_port = htons(port);
  if (host.empty() || host == "0.0.0.0") {
    out.sin_addr.s_addr = htonl(INADDR_ANY);
    return true;
  }
  if (::inet_pton(AF_INET, host.c_str(), &out.sin_addr) == 1) return true;
  // Fall back to name resolution.
  addrinfo hints{};
  hints.ai_family = AF_INET;
  hints.ai_socktype = SOCK_DGRAM;
  addrinfo* res = nullptr;
  if (::getaddrinfo(host.c_str(), nullptr, &hints, &res) != 0 || res == nullptr)
    return false;
  out.sin_addr = reinterpret_cast<sockaddr_in*>(res->ai_addr)->sin_addr;
  ::freeaddrinfo(res);
  return true;
}

// Split "host:port"; a bare ":port" or "port" means any-host.
inline bool split_hostport(const std::string& s, std::string& host, uint16_t& port) {
  auto colon = s.rfind(':');
  if (colon == std::string::npos) {
    host.clear();
    port = static_cast<uint16_t>(std::stoi(s));
    return true;
  }
  host = s.substr(0, colon);
  port = static_cast<uint16_t>(std::stoi(s.substr(colon + 1)));
  return true;
}

}  // namespace detail

// Sends pre-encoded DIFI packets over UDP.
class UdpSender {
 public:
  explicit UdpSender(const std::string& dest) {
    std::string host;
    uint16_t port;
    if (!detail::split_hostport(dest, host, port))
      throw DifiError("bad destination: " + dest);
    fd_ = ::socket(AF_INET, SOCK_DGRAM, 0);
    if (fd_ < 0) throw DifiError("socket() failed");
    if (!detail::resolve_v4(host, port, dest_)) {
      ::close(fd_);
      throw DifiError("cannot resolve " + host);
    }
  }
  ~UdpSender() {
    if (fd_ >= 0) ::close(fd_);
  }
  UdpSender(const UdpSender&) = delete;
  UdpSender& operator=(const UdpSender&) = delete;

  size_t send(const std::vector<uint8_t>& packet) {
    ssize_t n = ::sendto(fd_, packet.data(), packet.size(), 0,
                         reinterpret_cast<sockaddr*>(&dest_), sizeof(dest_));
    if (n < 0) throw DifiError(std::string("sendto failed: ") + std::strerror(errno));
    return static_cast<size_t>(n);
  }

 private:
  int fd_ = -1;
  sockaddr_in dest_{};
};

// Receives DIFI packets over UDP, optionally filtering by Stream ID.
class UdpReceiver {
 public:
  explicit UdpReceiver(const std::string& bind_addr) {
    std::string host;
    uint16_t port;
    if (!detail::split_hostport(bind_addr, host, port))
      throw DifiError("bad bind address: " + bind_addr);
    fd_ = ::socket(AF_INET, SOCK_DGRAM, 0);
    if (fd_ < 0) throw DifiError("socket() failed");
    sockaddr_in addr{};
    if (!detail::resolve_v4(host, port, addr)) {
      ::close(fd_);
      throw DifiError("cannot resolve bind host " + host);
    }
    if (::bind(fd_, reinterpret_cast<sockaddr*>(&addr), sizeof(addr)) < 0) {
      ::close(fd_);
      throw DifiError(std::string("bind failed: ") + std::strerror(errno));
    }
    buf_.resize(constants::MAX_PAYLOAD_OCTETS + 64);
  }
  ~UdpReceiver() {
    if (fd_ >= 0) ::close(fd_);
  }
  UdpReceiver(const UdpReceiver&) = delete;
  UdpReceiver& operator=(const UdpReceiver&) = delete;

  // The actual bound port (useful after binding to port 0).
  uint16_t local_port() const {
    sockaddr_in a{};
    socklen_t len = sizeof(a);
    if (::getsockname(fd_, reinterpret_cast<sockaddr*>(&a), &len) < 0) return 0;
    return ntohs(a.sin_port);
  }

  void set_stream_filter(bool enabled, uint32_t stream_id = 0) {
    filter_enabled_ = enabled;
    filter_id_ = stream_id;
  }

  // Timeout in milliseconds; 0 means block indefinitely.
  void set_read_timeout_ms(int ms) {
    timeval tv{};
    tv.tv_sec = ms / 1000;
    tv.tv_usec = (ms % 1000) * 1000;
    ::setsockopt(fd_, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
  }

  // Receive the next packet passing the Stream-ID filter. Off-stream packets are
  // dropped. Throws DifiError on timeout/error.
  std::vector<uint8_t> recv() {
    for (;;) {
      ssize_t n = ::recvfrom(fd_, buf_.data(), buf_.size(), 0, nullptr, nullptr);
      if (n < 0) {
        throw DifiError(std::string("recvfrom failed: ") + std::strerror(errno));
      }
      std::vector<uint8_t> packet(buf_.begin(), buf_.begin() + n);
      if (!filter_enabled_) return packet;
      uint32_t sid;
      if (peek_stream_id(packet, sid) && sid == filter_id_) return packet;
      // else: not our stream, keep receiving.
    }
  }

 private:
  int fd_ = -1;
  bool filter_enabled_ = false;
  uint32_t filter_id_ = 0;
  std::vector<uint8_t> buf_;
};

// ---------------------------------------------------------------------------
// TCP dev-mode transport (Req 5.2) — NON-STANDARD, length-framed.
// DIFI's transport is UDP; this is a development convenience for lossless, ordered
// exchange. Each DIFI packet is framed with a 4-byte big-endian length prefix.
// ---------------------------------------------------------------------------
namespace detail {
constexpr size_t TCP_MAX_FRAME = constants::MAX_PAYLOAD_OCTETS + 64;

inline bool read_n(int fd, uint8_t* buf, size_t n, bool& clean_eof) {
  size_t filled = 0;
  clean_eof = false;
  while (filled < n) {
    ssize_t r = ::recv(fd, buf + filled, n - filled, 0);
    if (r == 0) {
      if (filled == 0) { clean_eof = true; return false; }
      return false;  // EOF mid-frame
    }
    if (r < 0) {
      if (errno == EINTR) continue;
      return false;
    }
    filled += static_cast<size_t>(r);
  }
  return true;
}
}  // namespace detail

// Sends DIFI packets over a TCP connection, length-framed. Non-standard (dev only).
class TcpSender {
 public:
  explicit TcpSender(const std::string& dest) {
    std::string host;
    uint16_t port;
    if (!detail::split_hostport(dest, host, port))
      throw DifiError("bad destination: " + dest);
    fd_ = ::socket(AF_INET, SOCK_STREAM, 0);
    if (fd_ < 0) throw DifiError("socket() failed");
    sockaddr_in addr{};
    if (!detail::resolve_v4(host, port, addr)) {
      ::close(fd_);
      throw DifiError("cannot resolve " + host);
    }
    if (::connect(fd_, reinterpret_cast<sockaddr*>(&addr), sizeof(addr)) < 0) {
      ::close(fd_);
      throw DifiError(std::string("connect failed: ") + std::strerror(errno));
    }
  }
  ~TcpSender() {
    if (fd_ >= 0) ::close(fd_);
  }
  TcpSender(const TcpSender&) = delete;
  TcpSender& operator=(const TcpSender&) = delete;

  size_t send(const std::vector<uint8_t>& packet) {
    if (packet.size() > detail::TCP_MAX_FRAME)
      throw DifiError("packet exceeds TCP frame limit");
    uint32_t len = static_cast<uint32_t>(packet.size());
    uint8_t hdr[4] = {uint8_t(len >> 24), uint8_t(len >> 16), uint8_t(len >> 8),
                      uint8_t(len)};
    if (!send_all(hdr, 4) || !send_all(packet.data(), packet.size()))
      throw DifiError(std::string("send failed: ") + std::strerror(errno));
    return packet.size() + 4;
  }

 private:
  bool send_all(const uint8_t* buf, size_t n) {
    size_t sent = 0;
    while (sent < n) {
      ssize_t w = ::send(fd_, buf + sent, n - sent, 0);
      if (w < 0) {
        if (errno == EINTR) continue;
        return false;
      }
      sent += static_cast<size_t>(w);
    }
    return true;
  }
  int fd_ = -1;
};

// Receives length-framed DIFI packets over a single TCP connection. Non-standard.
class TcpReceiver {
 public:
  explicit TcpReceiver(const std::string& bind_addr) {
    std::string host;
    uint16_t port;
    if (!detail::split_hostport(bind_addr, host, port))
      throw DifiError("bad bind address: " + bind_addr);
    listen_fd_ = ::socket(AF_INET, SOCK_STREAM, 0);
    if (listen_fd_ < 0) throw DifiError("socket() failed");
    int one = 1;
    ::setsockopt(listen_fd_, SOL_SOCKET, SO_REUSEADDR, &one, sizeof(one));
    sockaddr_in addr{};
    if (!detail::resolve_v4(host, port, addr)) {
      ::close(listen_fd_);
      throw DifiError("cannot resolve bind host " + host);
    }
    if (::bind(listen_fd_, reinterpret_cast<sockaddr*>(&addr), sizeof(addr)) < 0 ||
        ::listen(listen_fd_, 1) < 0) {
      ::close(listen_fd_);
      throw DifiError(std::string("bind/listen failed: ") + std::strerror(errno));
    }
  }
  ~TcpReceiver() {
    if (conn_fd_ >= 0) ::close(conn_fd_);
    if (listen_fd_ >= 0) ::close(listen_fd_);
  }
  TcpReceiver(const TcpReceiver&) = delete;
  TcpReceiver& operator=(const TcpReceiver&) = delete;

  uint16_t local_port() const {
    sockaddr_in a{};
    socklen_t len = sizeof(a);
    if (::getsockname(listen_fd_, reinterpret_cast<sockaddr*>(&a), &len) < 0) return 0;
    return ntohs(a.sin_port);
  }

  void set_stream_filter(bool enabled, uint32_t stream_id = 0) {
    filter_enabled_ = enabled;
    filter_id_ = stream_id;
  }

  // Receive the next packet passing the filter. Accepts one connection on first call.
  // Returns an empty vector at clean end-of-stream.
  std::vector<uint8_t> recv() {
    if (conn_fd_ < 0) {
      conn_fd_ = ::accept(listen_fd_, nullptr, nullptr);
      if (conn_fd_ < 0) throw DifiError("accept failed");
    }
    for (;;) {
      uint8_t lenbuf[4];
      bool clean_eof = false;
      if (!detail::read_n(conn_fd_, lenbuf, 4, clean_eof)) {
        if (clean_eof) return {};  // end of stream at frame boundary
        throw DifiError("TCP read error / EOF mid-frame");
      }
      uint32_t len = (uint32_t(lenbuf[0]) << 24) | (uint32_t(lenbuf[1]) << 16) |
                     (uint32_t(lenbuf[2]) << 8) | uint32_t(lenbuf[3]);
      if (len > detail::TCP_MAX_FRAME) throw DifiError("TCP frame length exceeds limit");
      std::vector<uint8_t> packet(len);
      bool eof2 = false;
      if (!detail::read_n(conn_fd_, packet.data(), len, eof2))
        throw DifiError("TCP read error in frame body");
      if (!filter_enabled_) return packet;
      uint32_t sid;
      if (peek_stream_id(packet, sid) && sid == filter_id_) return packet;
      // else drop, keep reading
    }
  }

 private:
  int listen_fd_ = -1;
  int conn_fd_ = -1;
  bool filter_enabled_ = false;
  uint32_t filter_id_ = 0;
};

}  // namespace difi

#endif  // DIFI_DIFI_HPP
