// gr-difi receiver flowgraph (Route 2): uses gr-difi's OWN difi_source_cpp block to
// decode DIFI packets sent by our difi-tool export, and counts recovered samples.
//
// Flow: UDP in -> difi::difi_source_cpp -> vector_sink_c (capture) [+ null sink].

#include <gnuradio/top_block.h>
#include <gnuradio/blocks/vector_sink.h>
#include <gnuradio/blocks/head.h>
#include <difi/difi_source_cpp.h>

#include <chrono>
#include <complex>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <thread>

int main(int argc, char** argv) {
  std::string ip = argc > 1 ? argv[1] : "0.0.0.0";
  uint32_t port = argc > 2 ? (uint32_t)std::atoi(argv[2]) : 60100;
  int expect = argc > 3 ? std::atoi(argv[3]) : 4096;

  auto tb = gr::make_top_block("gr_difi_rx");
  // difi_source_cpp::make(ip, port, socket_type(0=UDP), stream_number, bit_depth,
  //   context_pkt_behavior)
  auto src = gr::difi::difi_source_cpp_fc32::make(ip, port, 0 /*UDP*/,
                                                  0x12345678 /*stream*/, 16 /*bit depth*/,
                                                  0 /*context behavior*/);
  auto head = gr::blocks::head::make(sizeof(gr_complex), expect);
  auto sink = gr::blocks::vector_sink_c::make();
  tb->connect(src, 0, head, 0);
  tb->connect(head, 0, sink, 0);

  std::fprintf(stderr, "gr-difi RX: listening on %s:%u for %d samples\n", ip.c_str(),
               port, expect);
  tb->start();
  // Wait until 'head' has passed 'expect' samples or we time out.
  auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(12);
  while (std::chrono::steady_clock::now() < deadline) {
    if ((int)sink->data().size() >= expect) break;
    std::this_thread::sleep_for(std::chrono::milliseconds(50));
  }
  tb->stop();
  tb->wait();

  size_t n = sink->data().size();
  std::fprintf(stderr, "gr-difi RX: recovered %zu samples\n", n);
  if ((int)n >= expect) {
    std::printf("GR-DIFI RX OK %zu\n", n);
    return 0;
  }
  std::printf("GR-DIFI RX INCOMPLETE %zu/%d\n", n, expect);
  return 1;
}
