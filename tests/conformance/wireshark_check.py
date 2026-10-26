#!/usr/bin/env python3
"""DIFI Wireshark dissector conformance check (spec task 10 / Req 3.1).

Validates that our packets parse cleanly in the **DIFI Consortium Wireshark dissector**
(`difi-dissector.lua`), run headless via `tshark`. This is the Req 3.1 check that the
`construct`-based `certify.py` (Req 3.2/3.3) does not cover.

Pipeline:
  1. Take the committed golden packets (or any dir of *.difi.bin).
  2. Wrap each DIFI UDP payload in Eth/IP/UDP via `text2pcap -u` -> a .pcap.
  3. Run `tshark -X lua_script:difi-dissector.lua` over the pcap, extracting DIFI fields.
  4. Assert the dissector recognised each packet as DIFI and parsed the expected fields.

The dissector registers as a UDP heuristic keyed on OUI 0x6A621E, so the chosen UDP port
does not matter.

Requires `tshark` and `text2pcap` (Wireshark). Auto-detected; override with DIFI_TSHARK
/ DIFI_TEXT2PCAP. Exits 0 if all packets dissect correctly, 1 otherwise, 3 if tooling or
the dissector is missing.
"""
import os
import shutil
import subprocess
import sys
import tempfile

_HERE = os.path.dirname(os.path.abspath(__file__))
_DISSECTOR = os.path.join(_HERE, ".difi-cert", "wireshark-dissector", "difi-dissector.lua")

_TSHARK_CANDIDATES = [
    os.environ.get("DIFI_TSHARK"),
    "/Applications/Wireshark.app/Contents/MacOS/tshark",
    shutil.which("tshark"),
    "/opt/homebrew/bin/tshark",
]
_TEXT2PCAP_CANDIDATES = [
    os.environ.get("DIFI_TEXT2PCAP"),
    "/Applications/Wireshark.app/Contents/MacOS/text2pcap",
    shutil.which("text2pcap"),
    "/opt/homebrew/bin/text2pcap",
]


def first_exe(cands):
    for c in cands:
        if c and os.path.exists(c) and os.access(c, os.X_OK):
            return c
    return None


def to_text2pcap_hex(data: bytes) -> str:
    """text2pcap offset+hex format: 'OFFSET  XX XX ...' per 16 bytes."""
    lines = []
    for off in range(0, len(data), 16):
        chunk = data[off:off + 16]
        hexpart = " ".join(f"{b:02x}" for b in chunk)
        lines.append(f"{off:06x}  {hexpart}")
    lines.append(f"{len(data):06x}")
    return "\n".join(lines) + "\n"


def build_pcap(text2pcap: str, payload: bytes, out_pcap: str):
    # -u SRC,DST synthesises a UDP header on the given ports; -t not needed.
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
        f.write(to_text2pcap_hex(payload))
        txt = f.name
    try:
        subprocess.run([text2pcap, "-q", "-u", "4991,4991", txt, out_pcap],
                       check=True, capture_output=True)
    finally:
        os.unlink(txt)


def dissect(tshark: str, pcap: str, fields):
    # Disable the built-in VITA 49 ("vrt") dissector so the DIFI Lua UDP heuristic
    # claims the packet (both register on UDP; vrt would otherwise win).
    cmd = [tshark, "-r", pcap, "-X", f"lua_script:{_DISSECTOR}",
           "--disable-protocol", "vrt", "-T", "fields"]
    for fld in fields:
        cmd += ["-e", fld]
    out = subprocess.run(cmd, capture_output=True, text=True)
    if out.returncode != 0:
        raise RuntimeError(f"tshark failed: {out.stderr.strip()}")
    # One line per packet; tab-separated fields.
    rows = [line.split("\t") for line in out.stdout.strip().splitlines() if line.strip()]
    return rows


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    golden_dir = sys.argv[1]

    tshark = first_exe(_TSHARK_CANDIDATES)
    text2pcap = first_exe(_TEXT2PCAP_CANDIDATES)
    if not tshark or not text2pcap:
        sys.stderr.write("SKIP: tshark/text2pcap not found (install Wireshark).\n")
        return 3
    if not os.path.exists(_DISSECTOR):
        sys.stderr.write("SKIP: DIFI dissector not fetched (run tests/conformance/setup.sh).\n")
        return 3

    # Expected: (file, expected packet type, expected stream id, extra field checks)
    cases = [
        ("signal_context_16bit.difi.bin", 4, "0x12345678",
         {"difi.packet_class_code": "0x0001"}),
        ("version_context.difi.bin", 5, "0x12345678",
         {"difi.packet_class_code": "0x0004"}),
        ("signal_data_16bit.difi.bin", 1, "0x12345678",
         {"difi.packet_class_code": "0x0000"}),
    ]

    base_fields = ["difi.packet_type", "difi.stream_id", "difi.oui",
                   "difi.packet_class_code"]
    overall_ok = True

    for fname, ptype, stream_hex, extra in cases:
        path = os.path.join(golden_dir, fname)
        if not os.path.exists(path):
            print(f"[MISS] {fname}: not found"); overall_ok = False; continue
        payload = open(path, "rb").read()

        with tempfile.NamedTemporaryFile(suffix=".pcap", delete=False) as pf:
            pcap = pf.name
        try:
            build_pcap(text2pcap, payload, pcap)
            fields = base_fields + [k for k in extra if k not in base_fields]
            rows = dissect(tshark, pcap, fields)
        finally:
            os.unlink(pcap)

        if len(rows) != 1:
            print(f"[FAIL] {fname}: dissector produced {len(rows)} packets (expected 1 — "
                  f"heuristic may not have matched)")
            overall_ok = False
            continue
        row = dict(zip(fields, rows[0] + [""] * (len(fields) - len(rows[0]))))

        def as_int(s):
            try:
                return int(s, 0) if isinstance(s, str) and s.strip() else None
            except ValueError:
                return None

        errs = []
        # Dissector recognised it as DIFI iff difi.* fields are populated.
        if as_int(row.get("difi.oui", "")) != 0x6A621E:
            errs.append(f"OUI not parsed as DIFI (got '{row.get('difi.oui')}')")
        if as_int(row.get("difi.packet_type", "")) != ptype:
            errs.append(f"packet_type '{row.get('difi.packet_type')}' != {ptype}")
        if as_int(row.get("difi.stream_id", "")) != int(stream_hex, 16):
            errs.append(f"stream_id '{row.get('difi.stream_id')}' != {stream_hex}")
        for k, v in extra.items():
            if as_int(row.get(k, "")) != int(v, 16):
                errs.append(f"{k} '{row.get(k)}' != {v}")

        if errs:
            print(f"[FAIL] {fname}:")
            for e in errs:
                print(f"         - {e}")
            overall_ok = False
        else:
            print(f"[PASS] {fname}: dissector parsed DIFI fields "
                  f"(type={row['difi.packet_type']}, stream={row['difi.stream_id']}, "
                  f"class={row.get('difi.packet_class_code')})")

    if overall_ok:
        print("WIRESHARK DISSECTOR: all golden packets dissect cleanly as DIFI")
        return 0
    print("WIRESHARK DISSECTOR: FAILURES above")
    return 1


if __name__ == "__main__":
    sys.exit(main())
