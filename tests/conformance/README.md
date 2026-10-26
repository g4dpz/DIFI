# DIFI Consortium Conformance Oracle (spec task 10 / Req 3)

This validates packets produced by our implementations against the **DIFI Consortium's
own reference packet definitions**, not just against our own other implementation. It is
the external oracle that establishes wire-level conformance (Req 3.2).

## What's here

- `certify.py` — **our** harness. It imports the DIFI Consortium's reference packet
  definitions and runs their field-level `validate()` methods against our golden
  packets, reporting pass/fail per packet (task 10 / Req 3.2). Covers the DIFI **1.1**
  data-plane packets (`difi_*_v1_1`) **and the DIFI 1.2 additions** — the sample-count
  Data/Context twins and the Timing Flow Control packets — against the Consortium's
  `difi_data_v1_2_1`, `difi_context_v1_2_1` and `difi_command_v1_2_1` reference
  definitions (`difi-1.2-dataplane` task 8).
- `cross_decode.py` — **our** harness for bidirectional cross-decode against the
  reference (task 13 / Req 7.2): the reference *builds* packets that our tools decode,
  and our tools' packets are parsed+validated by the reference. See
  `docs/interop/CROSS-DECODE-EVIDENCE.md`.
- `wireshark_check.py` — **our** harness for the DIFI Wireshark dissector check
  (Req 3.1): wraps our packets in a pcap (`text2pcap -u`) and runs `tshark` headless
  with the Consortium `difi-dissector.lua`, asserting each packet dissects as DIFI with
  the expected fields. Needs `tshark`/`text2pcap` (Wireshark); auto-detected (the macOS
  app bundle at `/Applications/Wireshark.app/Contents/MacOS/`), override with
  `DIFI_TSHARK`. The built-in `vrt` dissector is disabled so the DIFI UDP heuristic wins.
- `setup.sh` — fetches the Consortium definitions **and** the Wireshark dissector on
  demand, and builds the venv.

## Third-party code is fetched, not committed

The authoritative wire layouts are the DIFI Consortium's `construct` definitions from
[DIFI-Consortium/DIFI-Certification](https://github.com/DIFI-Consortium/DIFI-Certification)
(`packet_definitions/difi_*_v1_1.py`, `construct_custom_types.py`). That repository
carries **no explicit licence grant**, so this project does **not** redistribute those
files. Instead, `setup.sh` fetches them at a **pinned commit** into a gitignored local
cache (`tests/conformance/.difi-cert/`), and `certify.py` imports from there. Nothing
third-party is checked into this repo. The pinned commit lives in `setup.sh`; update it
deliberately.

## Running

```sh
tests/conformance/setup.sh                                   # fetch defs + build venv
/tmp/difi-venv/bin/python3 tests/conformance/certify.py tests/golden
```

Expected:

```
[PASS] Signal Context (108 bytes) — parses and validates clean
[PASS] Version Context (44 bytes) — parses and validates clean
[PASS] Signal Data (60 bytes) — parses and validates clean
DIFI CONFORMANCE: all packets pass the Consortium reference validation
```

`tests/interop.sh` runs this automatically (section 5) for both the Rust and C++ tools
when the venv is present.

## Why this mattered: a real conformance finding

Our first implementations passed all of our own tests and agreed byte-for-byte with each
other — yet the **Signal Context and Version Context packets were not DIFI-conformant**.
The external oracle is what caught it. The authoritative definitions require:

- **Signal Context**: a fixed **27-word** (108-byte) packet with VRT packet type `0x4`,
  `TSM=1`, `cif0` ∈ {`0xFBB98000`, `0x7BB98000`}, `refPoint=0x64`, fixed-point scaled
  fields (frequencies/bandwidth/sample-rate as integer·2²⁰; gains as integer·2⁷), a
  `stateEventInd` word, and a 2-word `dataPacketFormat` where the bit depth lives in
  `item_packing_field_size` and `data_item_size` as `(bits − 1)`.
- **Version Context**: a fixed **11-word** (44-byte) packet with VRT packet type **`0x5`**
  (not `0x4`), `cif1=0x0000000C`, `v49SpecVersion=0x00000004`, and a packed `versionInfo`
  word.

Our original guessed layouts (loose `f64` context fields; version as type `0x4`) were
internally consistent but wrong. Both implementations were rewritten to the conformant
layout; the golden packets were regenerated and now pass this oracle.

Takeaway: two independent implementations agreeing with each other proves *interop*, not
*standards conformance*. Only an external reference oracle proves the latter.

### And again for DIFI 1.2 (task 8)

Extending the oracle to the DIFI 1.2 packets (`difi_*_v1_2_1`) caught **three** more
conformance bugs that both implementations shared — byte-identical with each other, but
wrong against the standard:

1. **CIF0 CIF1-present bit.** We set bit 2; the v1.2.1 command reference requires **bit 1**
   (CIF0 `0x40300002`, not `0x40300004`). Our field-level transcription had it as bit 2.
2. **Information Class / Packet Class pairing.** The Timing Flow Control real-time packet
   (`0x0006`) must use Information Class `0x0003`, not `0x0002`; `0x0005` uses `0x0002`.
   We used `0x0002` for both.
3. **Sample Count Signal Context TSM.** Packet Class `0x0003` requires **TSM = 0** (fine),
   unlike the standard-flow context (`0x0001`) which uses TSM = 1. We emitted TSM = 1.

All three were fixed in the shared command core / encoders and the golden packets
regenerated; all seven packets now validate clean against the Consortium reference.

## Still not covered

- The **Wireshark dissector** (Req 3.1) is not run here (it needs a live Wireshark +
  loopback capture). The `construct` definitions in this repo are the same wire contract
  the dissector encodes, so passing them is strong evidence, but a dissector capture is a
  separate manual check worth doing before claiming full Req 3.1.
- The context fields we don't use (IF band offset, reference levels, timestamp
  adjust/cal, state/event indicators) are emitted as zeros. They pass validation but are
  not exercised with meaningful values.
