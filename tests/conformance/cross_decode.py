#!/usr/bin/env python3
"""Cross-decode against a second independent DIFI implementation (spec task 13 / Req 7.2, 7.3).

The "second implementation" here is the DIFI Consortium's own reference code — its
`construct` packet definitions (fetched by setup.sh). They are independent of ours
(different author, language, and parsing library) and are bidirectional: they can both
*build* and *parse* DIFI packets. This exercises cross-decode in BOTH directions:

  A. reference-builds -> ours-decodes : the reference code constructs packets from
     chosen field values; our difi-tool decodes them and must recover those values.
  B. ours-builds -> reference-parses  : our difi-tool emits packets; the reference code
     parses and validates them (extends the task-10 conformance check into an explicit
     cross-decode direction).

Direction A is the stronger interop claim: bytes authored entirely by the other
implementation, decoded by ours.

Usage:
    cross_decode.py <path-to-difi-tool>   # run against one of our tools (rust or cpp)

Exits 0 if every cross-decode passes, 1 otherwise. Prints a structured report.

NOTE: this is the DIFI *certification* reference, which defines the standard — a strong
independent check, but not a fully separate vendor stack like gr-difi. See
docs/interop/CROSS-DECODE-EVIDENCE.md.
"""
import os
import struct
import subprocess
import sys
import tempfile

_HERE = os.path.dirname(os.path.abspath(__file__))
_DEFS_ROOT = os.path.join(_HERE, ".difi-cert")
if not os.path.isdir(os.path.join(_DEFS_ROOT, "packet_definitions")):
    sys.stderr.write("Run tests/conformance/setup.sh first.\n")
    sys.exit(3)
sys.path.insert(0, _DEFS_ROOT)

from construct import Container
from packet_definitions.difi_data_v1_1 import difi_data_definition as DATA
from packet_definitions.difi_context_v1_1 import difi_context_definition as CTX
from packet_definitions.difi_version_v1_1 import difi_version_definition as VER

STREAM_ID = 0xABCD1234
INT_SECS = 1700000000
FRAC = 55


def ref_build_data(samples, seq):
    payload = b"".join(struct.pack(">hh", i, q) for (i, q) in samples)
    pkt = Container(
        header=Container(pktType=1, classId=1, reserved=0, tsm=0, tsi="UTC",
                         tsf=2, seqNum=seq, pktSize=7 + len(payload) // 4),
        streamId=STREAM_ID,
        classId=Container(paddingBits=0, reserved1=0, oui=0x6A621E,
                          infoClassCode=0, packetClassCode=0),
        intSecsTimestamp=INT_SECS, fracSecsTimestamp=FRAC, payload=payload)
    return DATA.build(pkt)


def run_dump(tool, path):
    out = subprocess.run([tool, "dump", path], capture_output=True, text=True)
    if out.returncode != 0:
        raise RuntimeError(f"dump failed: {out.stderr.strip()}")
    return out.stdout


def check(cond, msg, failures):
    if not cond:
        failures.append(msg)


def direction_a(tool):
    """reference builds -> our tool decodes -> verify recovered fields."""
    failures = []
    samples = [(10, -10), (20, -20), (30, -30), (40, -40), (0, 1), (-1, 0), (32767, -32768), (-5, 5)]
    raw = ref_build_data(samples, seq=3)
    with tempfile.NamedTemporaryFile(suffix=".bin", delete=False) as f:
        f.write(raw)
        path = f.name
    try:
        text = run_dump(tool, path)
    finally:
        os.unlink(path)

    check("stream=0xABCD1234" in text, "stream id not recovered", failures)
    check("seq=3" in text, "seq num not recovered", failures)
    check(f"samples={len(samples)}" in text, "sample count wrong", failures)
    for n, (i, q) in enumerate(samples):
        check(f"[{n}] I={i} Q={q}" in text, f"sample {n} mismatch (want I={i} Q={q})", failures)
    return failures, text


def direction_b(tool):
    """our tool builds (golden) -> reference parses + validates."""
    failures = []
    root = subprocess.check_output(["git", "-C", _HERE, "rev-parse", "--show-toplevel"]).decode().strip()
    with tempfile.TemporaryDirectory() as d:
        subprocess.run([tool, "gen-golden", d], check=True, capture_output=True)
        cases = [
            ("Signal Data", DATA, "signal_data_16bit.difi.bin"),
            ("Signal Context", CTX, "signal_context_16bit.difi.bin"),
            ("Version Context", VER, "version_context.difi.bin"),
        ]
        for label, defn, fname in cases:
            raw = open(os.path.join(d, fname), "rb").read()
            try:
                parsed = defn.parse(raw)
                errs = defn.validate(parsed)
            except Exception as e:
                failures.append(f"{label}: reference parse failed: {e}")
                continue
            if errs:
                failures.append(f"{label}: reference validation errors: {errs}")
    return failures


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    tool = sys.argv[1]
    impl = "rust" if "rust" in tool else ("cpp" if "cpp" in tool else tool)

    print(f"=== cross-decode: {impl} <-> DIFI Consortium reference ===")

    a_fail, a_text = direction_a(tool)
    if a_fail:
        print(f"[FAIL] A. reference-builds -> {impl}-decodes")
        for m in a_fail:
            print(f"         - {m}")
    else:
        print(f"[PASS] A. reference-builds -> {impl}-decodes  (8 samples incl. extremes recovered)")

    b_fail = direction_b(tool)
    if b_fail:
        print(f"[FAIL] B. {impl}-builds -> reference-parses")
        for m in b_fail:
            print(f"         - {m}")
    else:
        print(f"[PASS] B. {impl}-builds -> reference-parses+validates  (data, context, version)")

    if a_fail or b_fail:
        print("CROSS-DECODE: FAILURES above")
        return 1
    print("CROSS-DECODE: both directions pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
