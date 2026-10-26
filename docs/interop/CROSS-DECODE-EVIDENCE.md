# Interop Evidence — Cross-Decode vs an Independent DIFI Implementation

**Requirements:** 7.2 (demonstrate cross-decode in both directions with a second
independent DIFI implementation), 7.3 (record the result as structured evidence).

## What "second implementation" means here

The second implementation is the **DIFI Consortium's own reference code** — the
`construct` packet definitions from
[DIFI-Consortium/DIFI-Certification](https://github.com/DIFI-Consortium/DIFI-Certification)
(fetched on demand by `tests/conformance/setup.sh`). These are:

- **Independent of ours**: different author, different language (Python), different
  parsing library (`construct`). Neither implementation shares code with the other.
- **Bidirectional**: `construct` structs both *build* and *parse*, so the reference can
  author packets from scratch (not just validate ours).

**Honest scope.** This is the certification reference that *defines* the standard, not a
fully separate vendor product stack. It is a strong independent cross-check, but it is
not the same as interoperating with, e.g., **gr-difi** or a hardware digitizer. gr-difi
was not used: GNU Radio 3.10 is present on this machine, but gr-difi is not installed and
building it from source (GR dev toolchain, pybind) was out of scope for this step.
Cross-decode against gr-difi or hardware remains valuable future evidence.

## Directions tested (both pass, for both Rust and C++)

### A. reference-builds → ours-decodes  (the stronger claim)

The reference `construct` definition **builds** a Signal Data packet from chosen field
values — stream `0xABCD1234`, seq 3, eight complex samples including the 16-bit extremes
`(32767, -32768)`. Bytes authored entirely by the reference. Our `difi-tool dump`
decodes it and recovers every field exactly.

### B. ours-builds → reference-parses+validates

Our `difi-tool gen-golden` emits all three packet types (Signal Data, Signal Context,
Version Context). The reference definitions `parse()` them and run the Consortium's
`validate()` — zero errors.

## Result

```
=== cross-decode: rust <-> DIFI Consortium reference ===
[PASS] A. reference-builds -> rust-decodes  (8 samples incl. extremes recovered)
[PASS] B. rust-builds -> reference-parses+validates  (data, context, version)
CROSS-DECODE: both directions pass

=== cross-decode: cpp <-> DIFI Consortium reference ===
[PASS] A. reference-builds -> cpp-decodes  (8 samples incl. extremes recovered)
[PASS] B. cpp-builds -> reference-parses+validates  (data, context, version)
CROSS-DECODE: both directions pass
```

## Reproduce

```sh
tests/conformance/setup.sh      # fetch reference defs + venv (one-time)
/tmp/difi-venv/bin/python3 tests/conformance/cross_decode.py rust/target/debug/difi-tool
/tmp/difi-venv/bin/python3 tests/conformance/cross_decode.py cpp/build/difi-tool
# or: ./tests/interop.sh  (section 9)
```

## Remaining for stronger evidence

- Cross-decode against **gr-difi** (GNU Radio Source/Sink blocks) — a fully independent
  vendor stack. Requires building gr-difi against GNU Radio 3.10.
- Cross-decode against **vendor hardware** (e.g. a Kratos SpectralNet digitizer) — the
  eventual ESTRACK hardware-in-the-loop step.
- A **Wireshark dissector** capture (Req 3.1), still a separate manual check.
