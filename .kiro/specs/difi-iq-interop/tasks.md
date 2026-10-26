# Implementation Plan — DIFI I/Q Interoperability Toolkit

Tasks are ordered so each builds on the last and produces something testable. Each cites
the requirements it satisfies. DIFI 1.1 is the target; keep hooks for 1.2.1.

- [x] 1. Project skeleton and constants
  - Set up the chosen stack (default: Python package per design DQ-1) with test runner,
    lint, and the `.gitignore`/artifact policy already in place.
  - Add a `constants` module: VITA OUI `0x6A621E`, header length (7 words), the DIFI 1.1
    Information/Packet class codes, packet-type codes, timestamp-type (TSI/TSF) values.
  - _Requirements: 6.3_
  - **Done.** Two independent implementations instead of Python (DQ-1 resolved: Rust +
    C++). `rust/src/constants.rs`, `cpp/include/difi/difi.hpp` (`constants` namespace).

- [x] 2. Packet model — Signal Data
  - Implement `SignalDataPacket` with `pack()`/`unpack()`, big-endian, 32-bit-word
    aligned: header fields, Stream ID, OUI, class codes, integer + fractional (psec)
    timestamps, I/Q payload.
  - Unit test pack→unpack round-trip for the header and timestamps.
  - _Requirements: 1.1, 2.1_
  - **Done.** `rust/src/packet.rs`, `cpp/include/difi/difi.hpp`. Tests in
    `rust/tests/codec.rs`, `cpp/tests/test_codec.cpp`.

- [x] 3. I/Q payload packing for 4–16 bit signed integers
  - Implement sample (un)packing for all depths 4–16, including sub-byte packing with no
    zero padding; enforce whole-4-byte-word constraint.
  - Unit test bit-exact round-trip at representative depths (4, 8, 12, 16).
  - _Requirements: 1.6, 2.4_
  - **Done.** `rust/src/iq.rs` (MSB-first, I-then-Q). Bit-exact round-trip verified at
    all depths and under fuzzing (`rust/tests/fuzz.rs`).

- [x] 4. Packet model — Signal Context and Version Context
  - Implement both packets with `pack()`/`unpack()`: context metadata (RF freq,
    bandwidth, sample rate, gain, data format) and version fields.
  - Unit test round-trips.
  - _Requirements: 1.2, 1.3, 2.2_
  - **Done.** `rust/src/packet.rs`, `cpp/include/difi/difi.hpp`. First layout was a
    guess; corrected to the authoritative fixed 27-word context / 11-word version layout
    after task 10 (Version Context is VRT type 0x5; context uses 2^20/2^7 fixed-point).

- [x] 5. DIFI Decoder + validator (fail-closed)
  - Parse arbitrary bytes into the packet model; reject non-VITA OUI and
    Packet-Size/length mismatches with typed, field-level errors; bounds-check every
    field against received length before use.
  - Unit test: malformed inputs raise typed errors, never mis-parse or crash.
  - _Requirements: 1.4, 1.5, and security/robustness in design_
  - **Done.** `rust/src/decoder.rs`, `cpp/include/difi/difi.hpp`. Fuzzed with tens of
    thousands of malformed buffers (task 14); C++ clean under ASan+UBSan.

- [x] 6. DIFI Encoder with stream sequencing
  - Build valid Signal Data / Context packets from model + config; maintain mod-16
    sequence number across a data stream; reject invalid configs (word alignment,
    >~9000-octet payload).
  - Unit test: encode→decode round trip is bit-exact (Req 2.6); invalid configs rejected.
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6_
  - **Done.** `rust/src/encoder.rs`, `cpp/include/difi/difi.hpp` (`StreamEncoder`).
    Req 2.5 satisfied both ways: `encode_data` rejects an oversized single packet;
    `encode_data_segmented` splits a large buffer into word-aligned packets with running
    sequence numbers.

- [x] 7. Golden packets as fixtures
  - Produce a few KB-scale canonical DIFI packets with documented expected field values;
    commit them (allowlisted) as regression fixtures.
  - Test the decoder against each golden packet.
  - _Requirements: 3.4_
  - **Done.** `tests/golden/` (3 packets ≤108 B) + `MANIFEST.md` with field values and
    SHA-256. Both implementations generate them byte-identically (interop.sh §1) and the
    C++ build asserts a byte match (`golden_match` ctest).

- [x] 8. Sample Mapper (float ↔ DIFI integer)
  - Implement deterministic float32↔integer conversion with an explicit, documented
    scaling/reference-level convention (resolve DQ-2); add `measure_implementation_loss()`.
  - Unit test: forward/inverse mapping; report round-trip loss (do not assume zero).
  - _Requirements: 4.1, 4.2, 4.3_
  - **Done** (with a caveat). `rust/src/mapper.rs`, `cpp/include/difi/difi.hpp`. DQ-2
    resolved: 16-bit signed, full-scale = 2^(n-1)-1, documented in the golden manifest.
    Loss measured (max 0.5 LSB), never assumed zero. **Req 4.3 caveat:** the convention
    is documented but NOT yet reconciled against the DIFI MILCOM reference-gain/scaling
    guidance — follow-up.

- [x] 9. UDP transport (+ optional TCP dev mode)
  - Implement UDP send/receive decoupled from the packet model; Stream-ID filtering on
    receive; optional TCP mode flagged non-standard.
  - Integration test: two local endpoints exchange a short stream over UDP.
  - _Requirements: 5.1, 5.2, 5.3_
  - **Done.** `rust/src/transport.rs`, `cpp/include/difi/difi.hpp`. UDP send/recv +
    Stream-ID filtering (Req 5.1, 5.3), tested and live over 4 directions (interop.sh §3).
    Req 5.2 **also done**: `TcpSender`/`TcpReceiver`, length-framed, clearly non-standard,
    with Stream-ID filtering and loopback tests in both implementations.

- [x] 10. Conformance harness vs DIFI Consortium tooling
  - Scripts: our-encode → `certify_source.py` parse; `certify_sink.py` → our-decode;
    emit a capture for the DIFI Wireshark dissector.
  - Record pass/fail evidence.
  - _Requirements: 3.1, 3.2, 3.3_
  - **Done** (mostly). `tests/conformance/certify.py` validates our packets against the
    DIFI Consortium reference `construct` definitions (Req 3.2); `cross_decode.py` covers
    reference→ours decode (Req 3.3). Defs fetched on demand, pinned, not vendored (no
    licence to redistribute). **Req 3.1 (Wireshark dissector capture) NOT run** — needs a
    live Wireshark + loopback capture (manual GUI step). The construct defs encode the
    same wire contract the dissector does, so passing them is strong evidence.

- [x] 11. Signal generators + manifest writer
  - Generators reproduce reference signals from committed params; manifest writer emits
    SHA-256, size, sample format, DIFI context params, exact regen command; large output
    goes only to a gitignored working dir.
  - Test: regenerate → manifest SHA-256 matches.
  - _Requirements: 6.1, 6.2, 6.4_
  - **Done** (Req 6.1–6.3). `rust/src/signal.rs` + `rust/src/manifest.rs` +
    `rust/src/sha256.rs`; `difi-tool generate`/`verify-manifest`. Raw cf32 → gitignored
    `work/`, manifest → tracked `signals/`. Regenerate→SHA match tested
    (`rust/tests/manifest.rs`, interop.sh §7). **Req 6.4 partial:** manifest records
    SHA-256 for external hosting, but no actual Zenodo/Release dataset is wired up, and
    the committed signal is an AFS-*style* waveform, not a real LSIS-AFS vector (DQ-4).

- [x] 12. Interop: export/import an existing reference signal over DIFI
  - End-to-end: regenerate an LSIS-AFS float32 signal → map to DIFI integer → encode →
    UDP → decode → inverse-map → compare within documented quantization loss; record
    evidence.
  - _Requirements: 7.1, 7.3_
  - **Done.** `difi-tool export`/`import`; all four directions (incl. cpp↔rust) recover
    within the documented loss — end-to-end loss equals mapper-only loss (transport/codec
    add nothing). Tests in `rust/tests/export.rs`, interop.sh §5–6. Evidence:
    `docs/interop/EXPORT-IMPORT-EVIDENCE.md`. Uses an AFS-*style* waveform (see DQ-4).

- [x] 13. Cross-implementation interop (where a second impl is available)
  - Cross-decode with gr-difi (or another DIFI implementation) in both directions; record
    results as structured interop evidence suitable for the community hub.
  - _Requirements: 7.2, 7.3_
  - **Done**, three ways: (a) the DIFI Consortium reference defs, both directions
    (`tests/conformance/cross_decode.py`, `CROSS-DECODE-EVIDENCE.md`); (b) GNU Radio
    Route 1 — its runtime + socket_pdu, within 1 LSB, 4 directions (`tests/gnuradio/`,
    `GNURADIO-EVIDENCE.md`); (c) GNU Radio Route 2 — gr-difi's OWN blocks
    (`tests/gnuradio/route2_grdifi/`, `GNURADIO-GRDIFI-EVIDENCE.md`). **Route 2 caveats:**
    ours→gr-difi fully decodes; gr-difi→ours verified at packet/sample-count level (not
    end-to-end loss — gr-difi uses its own scaling); gr-difi's sink is flaky and needed
    macOS build patches. Vendor-hardware interop (ESTRACK HITL) remains future.

- [x] 14. Property/fuzz hardening (stretch)
  - Property test: random valid configs round-trip bit-exact. Fuzz the decoder: malformed
    input only ever raises typed errors.
  - _Requirements: robustness in design; strengthens 1.4, 1.5_
  - **Done.** `rust/tests/fuzz.rs` + C++ `test_codec.cpp` fuzz/property tests, seeded
    (reproducible), dependency-free. ~20k garbage buffers per run never panic/mis-parse;
    C++ fuzz run clean under AddressSanitizer + UBSan (`cmake -DDIFI_SANITIZE=ON`).

## Status summary

All 14 tasks are implemented in **two independent implementations (Rust + C++)** — not
the Python package DQ-1 proposed — validated against each other, against the DIFI
Consortium reference, and against gr-difi. See `IMPLEMENTATIONS.md` and `docs/interop/`.
Build/test: `cd rust && cargo test`; `cmake -S cpp -B cpp/build && cmake --build cpp/build
&& ctest --test-dir cpp/build`; `./tests/interop.sh` (9 sections).

### Known gaps (deferred, not silently dropped)
- **Req 3.1 — Wireshark dissector capture:** not run (manual/tooling step; the construct
  conformance check covers the same wire contract — Req 3.2/3.3 done). The DIFI `.lua`
  dissector exists in the cert repo and could be run headless via `tshark`.
- **Req 4.3 — scaling vs DIFI MILCOM reference-gain guidance:** convention is documented
  and pinned, but not yet reconciled against that external guidance.
- **Req 6.4 / DQ-4 — canonical dataset hosting:** manifest records SHA-256 for external
  hosting, but no real LSIS-AFS vector or Zenodo/Release asset is wired up; the committed
  signal is a reproducible AFS-*style* waveform.
- **Task 13 / gr-difi:** interop is verified at packet/sample-count level, not by an
  end-to-end loss round-trip (gr-difi uses its own scaling); gr-difi's sink is flaky.
- **Design doc:** DQ-1 (Rust+C++), DQ-2 (16-bit full-scale), DQ-3 (DIFI 1.1 only)
  resolved in practice; `design.md` still lists them as open.

## Notes
- Tasks 1–7 deliver a standalone, unit-tested DIFI 1.1 codec (no network needed).
- Tasks 8–10 add the float-signal bridge and external conformance.
- Tasks 11–13 deliver reproducible data and real interop evidence.
- 1.2.1 (flow control, command packets) is a future increment — the packet-type handling
  from tasks 1–6 should leave room for it without rework.
