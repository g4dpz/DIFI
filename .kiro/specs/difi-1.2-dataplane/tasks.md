# Implementation Plan — DIFI 1.2 Data Plane Upgrade (Rust + C++)

Tasks are ordered so each builds on the last and produces something testable. Each cites
the requirements it satisfies. Every packet/codec task is done in **both** the Rust crate
(`rust/src/`) and the C++ header-only library (`cpp/include/difi/difi.hpp`), kept in
lockstep and proven byte-identical. The work is **additive** — no existing DIFI 1.1
behaviour, layout, constant value, or passing test may change.

The authoritative Command-Packet layout is in the gitignored transcription
(`work/spec-transcription-1.3.0.md`, §4.4.2); keep all confidential text out of the repo.

- [x] 1. Regression baseline + extended constants
  - First, run the full existing 1.1 suite and record it green as the baseline that must
    stay green (Req 4.2): `cd rust && cargo test`; C++ `ctest`; `tests/interop.sh`.
  - Add the 1.2 class codes beside the 1.1 ones in both implementations, keeping
    Information-Class and Packet-Class codes in separate namespaces (they share numeric
    values but live in different packet fields):
    - `information_class`: `0x0002`, `0x0003`, `0x0004` (groupings)
    - `packet_class`: `0x0002` (Sample Count Signal Data), `0x0003` (Sample Count Signal
      Context), `0x0005`/`0x0006` (Timing Flow Control). NB `packet_class` `0x0004`
      (Version Context) already exists — do not duplicate or change it.
    - `packet_type`: Command `0x6`.
    - Buffer Status flag bit positions (OF/NF/NE/UF); Reference Point default `0x00000064`.
  - Unit test: the two implementations agree on every new constant; no 1.1 constant value
    changed; Information-Class vs Packet-Class codes are not conflated.
  - _Requirements: 3.5, 4.1, 4.2_
  - _Files: `rust/src/constants.rs`, `cpp/include/difi/difi.hpp` (`constants`)._

- [x] 2. Shared Command-Packet core (CAM / CIF0 / CIF1 / IDs)
  - Implement the shared `command` module/namespace: CAM field read/write (Controllee &
    Controller ID present bits, action/acknowledge mode), CIF0/CIF1 field-presence helpers,
    and the common context words (Message ID, Controllee ID, Controller ID, Reference
    Point, Sample Rate, Timestamp Adjustment, Buffer Size). Default unused IDs to
    `0x00000000`.
  - This is the foundation the 1.3.0 control plane will reuse — build it clean and
    well-tested.
  - Unit test: CAM/CIF bit round-trips; the two implementations set identical bits for the
    same inputs.
  - _Requirements: 1.2, 2.2_
  - _Files: `rust/src/command.rs`, `cpp/include/difi/difi.hpp` (`difi::command`)._

- [x] 3. Timing Flow Control packet (`0x0005`/`0x0006`) encode/decode
  - Implement the Timing Flow Control packet model on the `command` core: full 21-word base
    layout, Buffer Status flags (OF/NF/NE/UF), and the class-specific TSF interpretation
    (`0x0005` sample-count, `0x0006` real-time ps).
  - NOTE (decoder gate — from the 1.1-code review): the existing `Decoder` refuses to
    decode a Signal Data packet until it has seen a Signal Context for that Stream ID (it
    tracks `bit_depth_by_stream`). Command / flow-control packets carry no sample bit depth
    and MUST decode regardless of stream state. When adding the `0x0005`/`0x0006` dispatch
    arm, route command packets to the flow-control decoder **before** / independently of
    the bit-depth gate, so a flow-control packet decodes even when no Signal Context has
    been seen for its stream. Do not weaken the gate for Signal Data itself. (The existing
    `set_bit_depth()` hook is the seam the 1.3.0 state machine will use to seed format from
    negotiation — leave it in place.)
  - Unit test: pack→unpack round-trip for both classes incl. Buffer Status flags and TSF
    mode; fail-closed on OUI/class/size mismatch; the right TSF interpretation per class; a
    flow-control packet decodes with no prior Signal Context for its stream.
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 2.1, 2.2, 2.3, 2.4_
  - _Files: `rust/src/flowcontrol.rs`, `rust/src/decoder.rs`, `cpp/include/difi/difi.hpp`
    (`difi::flowcontrol`, decoder)._

- [x] 4. Sample-count Data/Context twins (`0x0002`/`0x0003`) + Information-Class routing
  - Implement Sample Count Signal Data (`0x0002`) and Sample Count Signal Context
    (`0x0003`) as thin variants of the existing Signal Data / Signal Context codecs: same
    field layout, class code set to `0x0002`/`0x0003`, TSF set to sample-count mode. Add
    matching encode support.
  - NOTE (codec reuse — from the 1.1-code review): `SignalDataPacket::unpack` currently
    hard-rejects any `packet_type != SIGNAL_DATA`, and `SignalContextPacket::unpack`
    hard-rejects any `packet_class != SIGNAL_CONTEXT` (same assertions in the C++
    `difi.hpp`). To reuse these codecs for the sample-count twins, relax those two checks to
    accept the twin class (e.g. an allowed-class predicate), rather than copy-pasting the
    codec. Keep them fail-closed for genuinely wrong classes. This is the only edit to
    existing 1.1 codec lines; it must not change behaviour for the `0x0000`/`0x0001`
    inputs (covered by the task-5 regression gate).
  - Extend the decoder dispatch (table-driven) to recognize Information Classes `0x0002`/
    `0x0003`/`0x0004` and Packet Classes `0x0002`/`0x0003`, routing each by Packet Class.
    Confirm Packet Class `0x0004` still routes to the existing Version Context path and is
    NOT re-implemented.
  - VERIFY DURING IMPLEMENTATION: the 1.3.0 Table 4-1 summary groups Version Flow Signal
    Context (`0x0004`) under VRT Packet Type `0x4` (Context), whereas our validated 1.1
    implementation uses VRT type `0x5` for the Version Context (matching the DIFI
    Consortium reference `construct` definitions and the Wireshark dissector). Do NOT
    change our Version Context on the strength of the Table 4-1 summary alone — confirm the
    actual VRT type against the reference definitions / dissector first; the summary table
    is coarser than the field-level definition. Record the finding either way.
  - Unit test: the sample-count twins round-trip and carry sample-count TSF; a 1.1 packet
    (incl. Version Context `0x0004`) still decodes exactly as before through the extended
    dispatch (Req 4.4).
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 4.4_
  - _Files: `rust/src/decoder.rs`, `rust/src/encoder.rs`, `rust/src/packet.rs`, C++ header._

- [ ] 5. Backward-compatibility regression gate
  - Re-run the full 1.1 suite and assert it is still green and the 1.1 golden packets are
    byte-identical (Req 4.2, 4.3). Add an explicit test that a canonical 1.1 Signal
    Data/Context/Version packet decodes identically before and after the 1.2 additions.
  - _Requirements: 4.1, 4.2, 4.3, 4.4_
  - _Files: `rust/tests/`, C++ tests, `tests/interop.sh`._

- [ ] 6. Golden packets for the new 1.2 classes
  - Produce committed KB-scale golden packets for `0x0005` and `0x0006`, plus one each for
    the sample-count twins `0x0002`/`0x0003` (same layout as `0x0000`/`0x0001` but
    sample-count TSF — the golden captures the class code + TSF mode), each with a
    documented field-value
    manifest and SHA-256.
  - Test: both implementations generate each byte-identically (Req 5.2) and the C++ build
    asserts a byte match, as the 1.1 `golden_match` ctest does; the decoder recovers the
    documented fields.
  - _Requirements: 5.2, 6.3_
  - _Files: `tests/golden/` (+ `MANIFEST.md`); generation in both tools._

- [ ] 7. Rust↔C++ interop: flow-controlled UDP exchange
  - Script a short flow-controlled stream over UDP both directions (Rust Sink↔C++ Source
    and the reverse): Source streams Signal Data, Sink emits Timing Flow Control packets,
    both interpret the buffer state consistently. Record structured evidence under
    `docs/interop/`.
  - Integration test / interop: run both directions over loopback UDP; assert the decoded
    flow-control fields match on both ends. Extend `tests/interop.sh` with a flow-control
    section.
  - _Requirements: 5.1, 5.3_
  - _Files: `rust/src/bin/`, `cpp/src` tool, `tests/interop.sh`, `docs/interop/`._

- [ ] 8. Conformance vs external tooling (where 1.2.1 is supported)
  - Extend `tests/conformance/` to validate the new packets against the DIFI reference
    definitions / Wireshark dissector where 1.2.1 is covered, run headless as in the 1.1
    harness. Where a packet is not yet supported externally, record the gap honestly and
    rely on the golden packets.
  - _Requirements: 6.1, 6.2_
  - _Files: `tests/conformance/` (extended)._

- [ ] 9. Close 1.1 gap — scaling vs DIFI reference-gain guidance
  - Reconcile the documented `ScalingConvention` (16-bit, full-scale = 2^(n-1)-1) against
    the DIFI MILCOM "Reference Gain and Scaling" guidance; confirm consistent or adjust;
    record the outcome in the manifest and in this feature's design. If adjusted, keep the
    mapper's measured-loss reporting and re-verify round-trip loss.
  - _Requirements: 7.1, 7.5_
  - _Files: `rust/src/mapper.rs` (if adjusted), `docs/`, signal manifest._

- [ ] 10. Close 1.1 gap — canonical dataset hosting decision
  - Make and record an explicit decision: either wire up a real externally-hosted canonical
    dataset (Zenodo/Release) referenced by SHA-256 from its manifest, or formally keep the
    reproducible AFS-style waveform as the committed reference with documented rationale.
    Update the manifest and docs so this is no longer a silent partial.
  - _Requirements: 7.2, 7.5_
  - _Files: signal manifest, `docs/`, relevant spec note._

- [ ] 11. Close 1.1 gap — gr-difi end-to-end loss (or precise limitation)
  - Either add an end-to-end loss round-trip with gr-difi that reconciles its own scaling,
    or document precisely what is and is not verified (packet/sample-count level vs
    end-to-end loss), replacing the implied-full claim. Record under `docs/interop/`.
  - _Requirements: 7.3, 7.5_
  - _Files: `tests/gnuradio/`, `docs/interop/`._

- [ ] 12. Close 1.1 gap — fold DQ-2/DQ-4 resolutions into the 1.1 design doc
  - The DQ-3 version-target wording in `.kiro/specs/difi-iq-interop/design.md` ("1.1 only")
    has **already been corrected** (it read as contradicting this feature and 1.3.0; now it
    states 1.1 is that feature's scope while the toolkit targets 1.3.0 via 1.2/1.3.0).
    DQ-1 and DQ-2 already read as resolved.
  - Remaining for this task: once tasks 9 (scaling vs reference-gain guidance) and 10
    (dataset-hosting decision) land, update the 1.1 design's DQ-2 caveat and DQ-4 to
    record their outcomes, so no DQ in the 1.1 design reads as an unresolved open question
    that is actually settled.
  - _Requirements: 7.4, 7.5_
  - _Files: `.kiro/specs/difi-iq-interop/design.md`._

- [ ] 13. Documentation + status sweep
  - Update `IMPLEMENTATIONS.md` / `README` to describe the 1.2 data plane and how to run
    the new packets; record the 1.2 interop evidence; refresh the "Implementation status"
    and "Known gaps" so every item is either closed or re-deferred with a specific reason.
    No confidential text in any committed file.
  - _Requirements: 6.2, 7.5_
  - _Files: `IMPLEMENTATIONS.md`, `README.md`, `docs/interop/`._

- [ ] 14. Property/fuzz hardening (stretch)
  - Property test: random valid flow-control configs round-trip bit-exact in both
    implementations. Fuzz the command/flow-control decoders: malformed buffers only ever
    raise typed errors, never panic/crash. Run the C++ fuzz clean under ASan+UBSan.
  - _Requirements: robustness in design; strengthens 1.4_
  - _Files: `rust/tests/`, C++ test/fuzz, `cmake -DDIFI_SANITIZE=ON`._

## Notes
- Tasks 1–4 deliver the 1.2 packets as a standalone, unit-tested codec, reusing the 1.1
  word codec and fail-closed decode, with the shared `command` core as the centrepiece.
- Task 5 is the hard backward-compatibility gate — the upgrade is only safe if the entire
  1.1 suite stays green and the 1.1 goldens stay byte-identical.
- Tasks 6–8 deliver byte-identical goldens, live Rust↔C++ flow-control interop, and
  external conformance where available.
- Tasks 9–12 close the four carried-over 1.1 gaps (scaling guidance, dataset hosting,
  gr-difi end-to-end, design-doc DQ wording); task 13 sweeps docs/status so nothing is a
  silent partial.
- The `command` core built in task 2 is the foundation the 1.3.0 link-establishment
  feature reuses; after this lands, `difi-1.3.0-exploration` rebases onto it.
- This feature intentionally stops short of 1.3.0 link establishment, live ESTRACK
  hardware, and full VITA 49.2 Command generality.
