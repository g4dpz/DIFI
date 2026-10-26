# Implementation Plan — DIFI I/Q Interoperability Toolkit

Tasks are ordered so each builds on the last and produces something testable. Each cites
the requirements it satisfies. DIFI 1.1 is the target; keep hooks for 1.2.1.

- [ ] 1. Project skeleton and constants
  - Set up the chosen stack (default: Python package per design DQ-1) with test runner,
    lint, and the `.gitignore`/artifact policy already in place.
  - Add a `constants` module: VITA OUI `0x6A621E`, header length (7 words), the DIFI 1.1
    Information/Packet class codes, packet-type codes, timestamp-type (TSI/TSF) values.
  - _Requirements: 6.3_

- [ ] 2. Packet model — Signal Data
  - Implement `SignalDataPacket` with `pack()`/`unpack()`, big-endian, 32-bit-word
    aligned: header fields, Stream ID, OUI, class codes, integer + fractional (psec)
    timestamps, I/Q payload.
  - Unit test pack→unpack round-trip for the header and timestamps.
  - _Requirements: 1.1, 2.1_

- [ ] 3. I/Q payload packing for 4–16 bit signed integers
  - Implement sample (un)packing for all depths 4–16, including sub-byte packing with no
    zero padding; enforce whole-4-byte-word constraint.
  - Unit test bit-exact round-trip at representative depths (4, 8, 12, 16).
  - _Requirements: 1.6, 2.4_

- [ ] 4. Packet model — Signal Context and Version Context
  - Implement both packets with `pack()`/`unpack()`: context metadata (RF freq,
    bandwidth, sample rate, gain, data format) and version fields.
  - Unit test round-trips.
  - _Requirements: 1.2, 1.3, 2.2_

- [ ] 5. DIFI Decoder + validator (fail-closed)
  - Parse arbitrary bytes into the packet model; reject non-VITA OUI and
    Packet-Size/length mismatches with typed, field-level errors; bounds-check every
    field against received length before use.
  - Unit test: malformed inputs raise typed errors, never mis-parse or crash.
  - _Requirements: 1.4, 1.5, and security/robustness in design_

- [ ] 6. DIFI Encoder with stream sequencing
  - Build valid Signal Data / Context packets from model + config; maintain mod-16
    sequence number across a data stream; reject invalid configs (word alignment,
    >~9000-octet payload).
  - Unit test: encode→decode round trip is bit-exact (Req 2.6); invalid configs rejected.
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6_

- [ ] 7. Golden packets as fixtures
  - Produce a few KB-scale canonical DIFI packets with documented expected field values;
    commit them (allowlisted) as regression fixtures.
  - Test the decoder against each golden packet.
  - _Requirements: 3.4_

- [ ] 8. Sample Mapper (float ↔ DIFI integer)
  - Implement deterministic float32↔integer conversion with an explicit, documented
    scaling/reference-level convention (resolve DQ-2); add `measure_implementation_loss()`.
  - Unit test: forward/inverse mapping; report round-trip loss (do not assume zero).
  - _Requirements: 4.1, 4.2, 4.3_

- [ ] 9. UDP transport (+ optional TCP dev mode)
  - Implement UDP send/receive decoupled from the packet model; Stream-ID filtering on
    receive; optional TCP mode flagged non-standard.
  - Integration test: two local endpoints exchange a short stream over UDP.
  - _Requirements: 5.1, 5.2, 5.3_

- [ ] 10. Conformance harness vs DIFI Consortium tooling
  - Scripts: our-encode → `certify_source.py` parse; `certify_sink.py` → our-decode;
    emit a capture for the DIFI Wireshark dissector.
  - Record pass/fail evidence.
  - _Requirements: 3.1, 3.2, 3.3_

- [ ] 11. Signal generators + manifest writer
  - Generators reproduce reference signals from committed params; manifest writer emits
    SHA-256, size, sample format, DIFI context params, exact regen command; large output
    goes only to a gitignored working dir.
  - Test: regenerate → manifest SHA-256 matches.
  - _Requirements: 6.1, 6.2, 6.4_

- [ ] 12. Interop: export/import an existing reference signal over DIFI
  - End-to-end: regenerate an LSIS-AFS float32 signal → map to DIFI integer → encode →
    UDP → decode → inverse-map → compare within documented quantization loss; record
    evidence.
  - _Requirements: 7.1, 7.3_

- [ ] 13. Cross-implementation interop (where a second impl is available)
  - Cross-decode with gr-difi (or another DIFI implementation) in both directions; record
    results as structured interop evidence suitable for the community hub.
  - _Requirements: 7.2, 7.3_

- [ ] 14. Property/fuzz hardening (stretch)
  - Property test: random valid configs round-trip bit-exact. Fuzz the decoder: malformed
    input only ever raises typed errors.
  - _Requirements: robustness in design; strengthens 1.4, 1.5_

## Notes
- Tasks 1–7 deliver a standalone, unit-tested DIFI 1.1 codec (no network needed).
- Tasks 8–10 add the float-signal bridge and external conformance.
- Tasks 11–13 deliver reproducible data and real interop evidence.
- 1.2.1 (flow control, command packets) is a future increment — the packet-type handling
  from tasks 1–6 should leave room for it without rework.
