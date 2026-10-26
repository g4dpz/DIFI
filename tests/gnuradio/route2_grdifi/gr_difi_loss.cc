// SPDX-License-Identifier: Apache-2.0
// gr-difi end-to-end loss harness (Route 2, task 11): decode our difi-tool export with
// gr-difi's OWN difi_source_cpp block and write the recovered samples to a cf32 file so
// the loss vs our original signal can be measured (reconciling scaling conventions).
//
// gr-difi's source does NO scaling on decode: unpack_16 returns gr_complex(re, im) with
// re/im the raw int16 codes cast to float (see work/gr-difi/lib/difi_source_cpp_impl.h).
// So for a 16-bit export using our convention (FS = 2^(n-1)-1 = 32767), gr-difi emits
// floats in ±32767. The companion compare script renormalises by 32767 to recover the
// ±1.0 originals and reports the end-to-end quantization loss.
//
// Flow: UDP in -> difi::difi_source_cpp -> head(expect) -> vector_sink_c -> write cf32.

#include <gnuradio/top_block.h>
#include <gnuradio/blocks/vector_sink.h>
#include <gnuradio/blocks/head.h>
#include <difi/difi_source_cpp.h>

#include <chrono>
#include <complex>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <string>
#include <thread>

int main(int argc, char** argv) {
  std::string ip = argc > 1 ? argv[1] : "0.0.0.0";
  uint32_t port = argc > 2 ? (uint32_t)std::atoi(argv[2]) : 60300;
  int expect = argc > 3 ? std::atoi(argv[3]) : 4096;
  std::string out_path = argc > 4 ? argv[4] : "/tmp/gr_difi_recovered.cf32";

  auto tb = gr::make_top_block("gr_difi_loss");
  auto src = gr::difi::difi_source_cpp_fc32::make(ip, port, 0 /*UDP*/,
                                                  0x12345678 /*stream*/, 16 /*bit depth*/,
                                                  0 /*context behavior*/);
  auto head = gr::blocks::head::make(sizeof(gr_complex), expect);
  auto sink = gr::blocks::vector_sink_c::make();
  tb->connect(src, 0, head, 0);
  tb->connect(head, 0, sink, 0);

  std::fprintf(stderr, "gr-difi LOSS: listening on %s:%u for %d samples\n", ip.c_str(),
               port, expect);
  tb->start();
  auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(12);
  while (std::chrono::steady_clock::now() < deadline) {
    if ((int)sink->data().size() >= expect) break;
    std::this_thread::sleep_for(std::chrono::milliseconds(50));
  }
  tb->stop();
  tb->wait();

  auto data = sink->data();
  size_t n = data.size();
  std::fprintf(stderr, "gr-difi LOSS: recovered %zu samples\n", n);
  if ((int)n < expect) {
    std::printf("GR-DIFI LOSS INCOMPLETE %zu/%d\n", n, expect);
    return 1;
  }
  // Write the first `expect` recovered samples as interleaved LE float32 (cf32).
  std::ofstream f(out_path, std::ios::binary);
  for (int i = 0; i < expect; ++i) {
    float re = data[i].real(), im = data[i].imag();
    f.write(reinterpret_cast<const char*>(&re), 4);
    f.write(reinterpret_cast<const char*>(&im), 4);
  }
  std::printf("GR-DIFI LOSS OK %zu %s\n", n, out_path.c_str());
  return 0;
}
