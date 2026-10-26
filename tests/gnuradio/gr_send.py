#!/usr/bin/env python3
"""GNU Radio sender for the interop test (GNU Radio -> ours).

A GNU Radio flowgraph builds DIFI Signal Data packets (DIFI Consortium reference
definitions) carrying the reference signal, and streams them out over UDP via
`network.socket_pdu`. Our `difi-tool import` receives, decodes, and verifies.

GNU Radio's runtime and its UDP (socket_pdu) block are genuinely in the send path.

Usage (run with the GNU Radio venv python):
    gr_send.py <host:port-as-port>   # sends to 127.0.0.1:<port>

Note: our difi-tool import also expects a Signal Context first. The Consortium data
definition alone is enough for import's data decode (import tolerates leading context),
but import requires at least one context packet; we send a context via difi-tool on the
ours side is not possible here, so this sender also emits a context packet built from the
reference definition. See the runner.
"""
import sys
import os
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import difi_gr_common as c  # noqa: E402

from gnuradio import gr, network  # noqa: E402
import pmt  # noqa: E402
import threading  # noqa: E402


class PduSequenceSource(gr.basic_block):
    """Emits a fixed list of pre-built DIFI packets as PDUs, once, on start."""

    def __init__(self, payloads, gap_s=0.004):
        gr.basic_block.__init__(self, "difi_pdu_source", in_sig=[], out_sig=[])
        self.payloads = payloads
        self.gap_s = gap_s
        self.message_port_register_out(pmt.intern("pdus"))
        self._thread = None
        self.sent = 0

    def start(self):
        self._thread = threading.Thread(target=self._emit, daemon=True)
        self._thread.start()
        return True

    def _emit(self):
        time.sleep(0.3)  # let the UDP socket bind before sending
        for raw in self.payloads:
            vec = pmt.init_u8vector(len(raw), list(raw))
            self.message_port_pub(pmt.intern("pdus"), pmt.cons(pmt.PMT_NIL, vec))
            self.sent += 1
            time.sleep(self.gap_s)  # avoid loopback buffer overflow on bursts


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    port = sys.argv[1]

    # Pre-build all packets: 1 Signal Context + chunked Signal Data.
    iq = c.generate_signal()
    codes = c.map_signal_to_codes(iq)
    packets = [c.build_context_packet()]
    seq = 0
    for off in range(0, len(codes), c.SAMPLES_PER_PACKET):
        chunk = codes[off:off + c.SAMPLES_PER_PACKET]
        packets.append(c.build_data_packet(chunk, seq))
        seq = (seq + 1) & 0xF

    tb = gr.top_block()
    src = PduSequenceSource(packets)
    client = network.socket_pdu("UDP_CLIENT", "127.0.0.1", port, 10000, False)
    tb.msg_connect(src, "pdus", client, "pdus")

    tb.start()
    # Wait until all packets emitted plus a drain margin.
    deadline = time.time() + 10.0
    while time.time() < deadline and src.sent < len(packets):
        time.sleep(0.05)
    time.sleep(0.4)
    tb.stop()
    tb.wait()
    print(f"GR sent 1 context + {len(packets) - 1} data packets to 127.0.0.1:{port}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
