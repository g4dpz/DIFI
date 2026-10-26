// SPDX-License-Identifier: Apache-2.0
// gr-difi transmitter flowgraph (Route 2): uses gr-difi's OWN difi_sink_cpp block to
// encode a signal to DIFI and stream it over UDP to our difi-tool import.
//
// This exercises gr-difi's independent DIFI codec (not the Consortium reference).
//
// Flow: vector_source_c (our reference signal) -> difi::difi_sink_cpp -> UDP out.

#include <gnuradio/top_block.h>
#include <gnuradio/blocks/vector_source.h>
#include <difi/difi_sink_cpp.h>

#include <cmath>
#include <complex>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

// Reproduce the reference signal (mirrors signal.rs / difi_gr_common.py).
static std::vector<gr_complex> reference_signal() {
  const size_t N = 4096;
  const double fs = 1.25e6, t1 = 50000.0, t2 = -120000.0, peak = 0.9;
  const double TAU = 2.0 * M_PI;
  double w1 = TAU * t1 / fs, w2 = TAU * t2 / fs;
  std::vector<std::pair<double, double>> raw(N);
  double maxmag = 0.0;
  for (size_t k = 0; k < N; ++k) {
    double t = (double)k;
    double env = 0.75 + 0.25 * std::cos(TAU * k / N);
    double re = env * (std::cos(w1 * t) + 0.6 * std::cos(w2 * t));
    double im = env * (std::sin(w1 * t) + 0.6 * std::sin(w2 * t));
    raw[k] = {re, im};
    maxmag = std::max(maxmag, std::hypot(re, im));
  }
  double scale = maxmag > 0 ? peak / maxmag : 0.0;
  std::vector<gr_complex> out(N);
  for (size_t k = 0; k < N; ++k)
    out[k] = gr_complex((float)(raw[k].first * scale), (float)(raw[k].second * scale));
  return out;
}

int main(int argc, char** argv) {
  std::string ip = argc > 1 ? argv[1] : "127.0.0.1";
  uint32_t port = argc > 2 ? (uint32_t)std::atoi(argv[2]) : 60000;

  auto sig = reference_signal();
  auto tb = gr::make_top_block("gr_difi_tx");
  auto src = gr::blocks::vector_source_c::make(sig, false /*repeat*/, 1);

  // difi_sink_cpp::make(ref_time_full, ref_time_frac, ip, port, socket_type(0=UDP),
  //   mode, samples_per_packet, stream_number, samp_rate, context_interval,
  //   context_pack_size, bit_depth, scaling, gain, offset, max_iq, min_iq)
  auto sink = gr::difi::difi_sink_cpp_fc32::make(
      1700000000u, 123456789000ull, ip, port, 0 /*UDP*/, true /*standard mode*/,
      256 /*samples/pkt*/, 0x12345678 /*stream*/, 1250000ull /*samp_rate*/,
      1 /*context interval (pkts)*/, 27 /*context pack size*/, 16 /*bit depth*/,
      0 /*scaling: 0=manual*/, 1.0f /*gain*/, gr_complex(0, 0) /*offset*/,
      1.0f /*max_iq*/, -1.0f /*min_iq*/);

  tb->connect(src, 0, sink, 0);
  std::fprintf(stderr, "gr-difi TX: sending %zu samples to %s:%u\n", sig.size(),
               ip.c_str(), port);
  tb->run();
  std::fprintf(stderr, "gr-difi TX: done\n");
  return 0;
}
