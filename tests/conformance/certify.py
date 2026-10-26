#!/usr/bin/env python3
"""DIFI conformance oracle (spec task 10 / Req 3.2).

Validates packets produced by our implementations against the DIFI Consortium's own
reference packet definitions (the `construct` structs + `.validate()` methods from
github.com/DIFI-Consortium/DIFI-Certification, vendored under packet_definitions/).

This is the external oracle: it parses our bytes with the standard's reference code and
runs the Consortium's field-level validations. A packet is conformant only if it both
parses at the exact expected size AND produces zero validation errors.

Usage:
    certify.py <dir-with-golden-packets>

Expects these files in the directory:
    signal_context_16bit.difi.bin
    version_context.difi.bin
    signal_data_16bit.difi.bin

Exits 0 if every packet is conformant, 1 otherwise, printing a per-packet report.
"""
import sys
import os

# The DIFI Consortium reference definitions are fetched on demand by setup.sh into a
# gitignored local cache (they are third-party code and are not redistributed here).
_HERE = os.path.dirname(os.path.abspath(__file__))
_DEFS_ROOT = os.path.join(_HERE, ".difi-cert")

if not os.path.isdir(os.path.join(_DEFS_ROOT, "packet_definitions")):
    sys.stderr.write(
        "DIFI reference definitions not found.\n"
        "Run tests/conformance/setup.sh first (it fetches them + installs 'construct').\n"
    )
    sys.exit(3)

sys.path.insert(0, _DEFS_ROOT)

from packet_definitions.difi_context_v1_1 import difi_context_definition
from packet_definitions.difi_data_v1_1 import difi_data_definition
from packet_definitions.difi_version_v1_1 import difi_version_definition


def check_fixed_size(name, definition, data):
    """Context and version packets have a fixed size; data packets do not."""
    expected = definition.sizeof()
    if len(data) != expected:
        return [f"size {len(data)} != expected {expected}"]
    return []


def certify_one(label, definition, data, fixed_size):
    errors = []
    if fixed_size:
        errors += check_fixed_size(label, definition, data)
    try:
        parsed = definition.parse(data)
    except Exception as e:  # construct parse failure
        return [f"parse failed: {e}"]
    errors += definition.validate(parsed)
    return errors


def validate_data_size(data):
    """Signal data: packet-size field (words) must equal the actual length."""
    import struct
    (word0,) = struct.unpack(">I", data[0:4])
    pkt_size_words = word0 & 0xFFFF
    if pkt_size_words * 4 != len(data):
        return [f"declared {pkt_size_words} words != actual {len(data)} bytes"]
    return []


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    d = sys.argv[1]

    cases = [
        ("Signal Context", difi_context_definition,
         os.path.join(d, "signal_context_16bit.difi.bin"), True, None),
        ("Version Context", difi_version_definition,
         os.path.join(d, "version_context.difi.bin"), True, None),
        ("Signal Data", difi_data_definition,
         os.path.join(d, "signal_data_16bit.difi.bin"), False, validate_data_size),
    ]

    overall_ok = True
    for label, definition, path, fixed, extra in cases:
        if not os.path.exists(path):
            print(f"[MISS] {label}: file not found: {path}")
            overall_ok = False
            continue
        with open(path, "rb") as f:
            data = f.read()
        errors = certify_one(label, definition, data, fixed)
        if extra is not None:
            errors += extra(data)
        if errors:
            overall_ok = False
            print(f"[FAIL] {label} ({len(data)} bytes):")
            for e in errors:
                print(f"         - {e}")
        else:
            print(f"[PASS] {label} ({len(data)} bytes) — parses and validates clean")

    if overall_ok:
        print("DIFI CONFORMANCE: all packets pass the Consortium reference validation")
        return 0
    print("DIFI CONFORMANCE: FAILURES above")
    return 1


if __name__ == "__main__":
    sys.exit(main())
