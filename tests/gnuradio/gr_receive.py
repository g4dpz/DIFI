#!/usr/bin/env python3
"""GNU Radio receiver for the interop test (ours -> GNU Radio).

A GNU Radio flowgraph receives DIFI datagrams on UDP via `network.socket_pdu`, parses
each with the DIFI Consortium reference definitions, inverse-maps to float, and compares
against the regenerated reference signal within the documented quantization loss.

GNU Radio's runtime and its UDP (socket_pdu) block are genuinely in the receive path.

Usage (run with the GNU Radio venv python):
    gr_receive.py <bind-port> <expected-data-packets>

Prints "GR RECV OK <n>" on success (exit 0), or a diagnostic + exit 1.
"""
import sys
import os
import time
import threading

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import difi_gr_common as c  # noqa: E402

from gnuradio import gr, network  # noqa: E402
import pmt  # noqa: E402

# One LSB in the 16-bit full-scale float domain; interop is judged within this bound
# (cross-language float rounding can differ by ~1 LSB on isolated samples).
ONE_LSB = c.FULL_SCALE / ((1 << (c.BIT_DEPTH - 1)) - 1)
FS = (1 << (c.BIT_DEPTH - 1)) - 1


class Collector(gr.basic_block):
    """Message sink: collects DIFI data-packet samples from socket_pdu PDUs."""

    def __init__(self):
        gr.basic_block.__init__(self, "collector", in_sig=[], out_sig=[])
        self.codes = []          # recovered integer codes, in arrival order
        self.data_packets = 0
        self.lock = threading.Lock()
        self.message_port_register_in(pmt.intern("pdus"))
        self.set_msg_handler(pmt.intern("pdus"), self.handle)

    def handle(self, msg):
        try:
            vec = pmt.cdr(msg)
            raw = bytes(pmt.u8vector_elements(vec))
            parsed = c.parse_data_samples(raw)
        except Exception:
            return
        if parsed is None:
            return
        _seq, samples = parsed
        with self.lock:
            self.codes.extend(samples)
            self.data_packets += 1


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    port = sys.argv[1]
    expected_packets = int(sys.argv[2])

    tb = gr.top_block()
    server = network.socket_pdu("UDP_SERVER", "127.0.0.1", port, 10000, False)
    coll = Collector()
    tb.msg_connect(server, "pdus", coll, "pdus")
    tb.start()

    # Wait until we have all data packets (or time out).
    deadline = time.time() + 15.0
    total_codes = c.NUM_SAMPLES
    while time.time() < deadline:
        with coll.lock:
            if len(coll.codes) >= total_codes:
                break
        time.sleep(0.05)
    tb.stop()
    tb.wait()

    recovered_codes = coll.codes[:total_codes]
    if len(recovered_codes) < total_codes:
        print(f"GR RECV FAIL: got {len(recovered_codes)}/{total_codes} samples "
              f"({coll.data_packets} data packets)")
        return 1

    # Inverse-map and compare against the regenerated original within 1 LSB.
    original = c.generate_signal()
    max_abs = 0.0
    for (oi, oq), (ci, cq) in zip(original, recovered_codes):
        ri = ci / FS * c.FULL_SCALE
        rq = cq / FS * c.FULL_SCALE
        max_abs = max(max_abs, abs(ri - oi), abs(rq - oq))

    ok = max_abs <= ONE_LSB * 1.001
    print(f"GR received {len(recovered_codes)} samples in {coll.data_packets} data packets")
    print(f"  1 LSB = {ONE_LSB:.3e}, recovered-vs-original max_abs = {max_abs:.3e}")
    if ok:
        print(f"GR RECV OK {coll.data_packets}")
        return 0
    print("GR RECV FAIL: recovered signal exceeds 1 LSB tolerance")
    return 1


if __name__ == "__main__":
    sys.exit(main())
