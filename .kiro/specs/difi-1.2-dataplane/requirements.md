# Requirements — DIFI 1.2 Data Plane Upgrade (Rust + C++)

## Introduction

This feature upgrades the existing two-implementation toolkit (Rust library crate `difi`
+ C++17 header-only `difi/difi.hpp`) from the **DIFI 1.1** profile to the **DIFI 1.2.1**
data plane. It adds the packet families introduced across the 1.2.x revisions and closes
the known gaps carried over from the 1.1 implementation.

Per the DIFI Consortium's published version history, the 1.2 line added:
- **1.2.0 (Aug 2023)** — a standard **flow-control** mechanism, enabled by a new
  Information Class `0x0002` and new Packet Classes `0x0002`, `0x0003`, `0x0005`, and
  `0x0006`; plus a document restructure and a table of deviations from VITA 49.2.
- **1.2.1 (Feb 2025)** — clarifications from Plugfest Europe 2024 and a new Information
  Class `0x0004`.

Two of the new packet classes (`0x0005`, `0x0006`) are **Timing Flow Control** packets —
VITA 49.2 Command Packets (VRT type `0x6`). These are the same Command/Extension-Command
packet family that the later 1.3.0 link-establishment packets extend, so building them
here establishes the shared foundation (CAM field, CIF0/CIF1 indicators,
Controllee/Controller IDs, Buffer Size) once, for reuse by the 1.3.0 control plane.

This feature is **additive and backward-compatible**: it must not break the 1.1
data-plane behaviour, layouts, public API semantics, or any passing test. The target is
wire-level correctness, independent Rust↔C++ interoperability, and validation against the
DIFI Consortium tooling where it covers 1.2.1.

> Confidentiality: the field-level basis for the Command/Timing-Flow-Control packets is an
> engineering transcription of the confidential 1.3.0 specification kept in the gitignored
> `work/` directory. The public repo — these documents and all code — describes behaviour
> and structure in the project's own words and must not reproduce the standard's text.
> Where 1.2.1-specific layouts are needed, they are confirmed against the public 1.2.1
> materials and the DIFI certification reference definitions.

## Glossary

- **Flow control** — the 1.2.0 mechanism letting a Sink convey buffer fill level and
  timing to a Source so the Source can pace its stream (replacing proprietary buffer
  management).
- **Timing Flow Control Packet** — Packet Classes `0x0005` (Sample Count) and `0x0006`
  (Real-Time TSF); VITA 49.2 Command Packets (VRT type `0x6`) carrying buffer-status and
  timing fields.
- **Command Packet** — the VITA 49.2 packet family (VRT type `0x6`, or `0x7` for
  extension) that carries a CAM field, CIF0/CIF1, Message ID, and Controllee/Controller
  IDs. The shared base for flow control (1.2) and link establishment (1.3.0).
- **CAM** — Control/Acknowledge Mode field (VITA 49.2 §8), a 32-bit word selecting the
  control/acknowledge semantics of a Command Packet.
- **CIF0 / CIF1** — Control Indicator Fields; bitmaps declaring which optional context/
  command fields are present.
- **Buffer Size / Buffer Status** — the Sink's buffer capacity (bytes) and fill-level
  flags (Overflow / Near-Full / Near-Empty / Underflow) carried by a flow-control packet.
- **Information Class** — tells a receiver what other streams to expect and what the
  system is doing; **Packet Class** tells it how to parse a given packet's bits.

## Requirements

### Requirement 1 — Timing Flow Control packets (decode)

**User Story:** As an implementer, I want to decode Timing Flow Control packets, so that a
Source can react to a Sink's buffer state and timing.

#### Acceptance Criteria
1. WHEN a Timing Flow Control packet of Packet Class `0x0005` (Sample Count) or `0x0006`
   (Real-Time TSF) is received THEN the toolkit SHALL decode its header (VRT type `0x6`,
   Class-ID bit, control indicators, TSI, TSF, mod-16 sequence number, Packet Size),
   Stream ID, OUI, Information/Packet Class, integer- and fractional-seconds timestamps.
2. WHEN such a packet is decoded THEN the toolkit SHALL recover its Command-Packet context
   fields: CAM, Message ID, Controllee ID, Controller ID, CIF0, CIF1, Reference Point,
   Sample Rate, Timestamp Adjustment, Buffer Size, and the Buffer Status flags (Overflow /
   Near-Full / Near-Empty / Underflow).
3. WHEN the Packet Class is `0x0005` THEN the toolkit SHALL interpret TSF as sample-count;
   WHEN `0x0006` THEN the toolkit SHALL interpret TSF as real-time picoseconds.
4. WHEN a Timing Flow Control packet's OUI is not `0x6A621E`, or its Packet Class is not a
   recognized 1.2 class, or its Packet Size does not match the received length THEN the
   toolkit SHALL reject it with a typed, field-level error rather than mis-parsing.

### Requirement 2 — Timing Flow Control packets (encode)

**User Story:** As an implementer, I want to encode Timing Flow Control packets, so that a
Sink can report buffer state and timing to a Source.

#### Acceptance Criteria
1. WHEN buffer state, timing, and stream parameters are provided THEN the toolkit SHALL
   produce a valid Timing Flow Control packet of the requested class (`0x0005` or
   `0x0006`) with correct header, CAM, CIF0/CIF1, IDs, Reference Point, Sample Rate,
   Timestamp Adjustment, Buffer Size, Buffer Status flags, and Packet Size.
2. WHEN encoding THEN the toolkit SHALL set the CAM and CIF0/CIF1 bits correctly for the
   class (e.g. action mode, Controllee/Controller ID present bits, field-presence bits).
3. WHEN a Timing Flow Control packet is encoded and then decoded (round trip) THEN every
   recovered field SHALL match the inputs exactly.
4. WHEN a requested configuration cannot produce a well-formed packet THEN the toolkit
   SHALL reject it with a clear error rather than emit a malformed packet.

### Requirement 3 — Additional standard-flow classes (1.2.0 / 1.2.1)

**User Story:** As an implementer, I want to handle the additional Information and Packet
Classes added in 1.2.0/1.2.1, so that my device interoperates with 1.2.1 peers across the
full class set.

#### Acceptance Criteria
1. WHEN a packet carries Information Class `0x0002` (flow-control) or `0x0004` (added in
   1.2.1) THEN the toolkit SHALL recognize the class and route the packet to the correct
   decode path rather than rejecting it as unknown.
2. WHEN a packet carries Packet Class `0x0002` or `0x0003` THEN the toolkit SHALL decode
   it per its layout, distinguishing the standard-flow data/context parallels from the
   1.1 classes (`0x0000`/`0x0001`/`0x0004`) — where these differ from 1.1 primarily in
   timestamp interpretation (seconds vs sample count), the toolkit SHALL apply the correct
   interpretation.
3. WHEN encoding a stream that uses a 1.2 Information/Packet Class THEN the toolkit SHALL
   emit the correct class codes and the matching timestamp mode.
4. The class codes (Information and Packet) SHALL be defined as named constants in both
   implementations, identical across them, alongside the existing 1.1 codes.

### Requirement 4 — Backward compatibility with 1.1

**User Story:** As an existing user of the toolkit, I want the 1.1 data plane to keep
working unchanged, so that the upgrade is safe to adopt.

#### Acceptance Criteria
1. The feature SHALL NOT change the wire layout, field semantics, or public API behaviour
   of the existing 1.1 Signal Data, Signal Context, or Version Context packets.
2. All existing 1.1 tests (Rust `cargo test`, C++ `ctest`, `tests/interop.sh`, golden
   packet byte-match) SHALL continue to pass unchanged.
3. The existing 1.1 golden packets SHALL remain byte-identical; no regeneration that
   alters their bytes.
4. WHEN a decoder receives a 1.1 packet THEN it SHALL decode it exactly as before,
   regardless of the added 1.2 support.

### Requirement 5 — Rust ↔ C++ interoperability

**User Story:** As the community, we want the 1.2 additions proven across two independent
implementations, not just within one.

#### Acceptance Criteria
1. WHEN the Rust implementation encodes each new 1.2 packet THEN the C++ implementation
   SHALL decode it and recover identical field values, and vice versa.
2. WHEN both implementations encode the same 1.2 packet from the same inputs THEN the byte
   output SHALL be identical (byte-for-byte golden match), as done for the 1.1 packets.
3. WHEN a Rust Sink and a C++ Source (and the reverse) exchange a short stream that
   includes Timing Flow Control packets over UDP THEN both SHALL interpret the flow-control
   state consistently, and the result SHALL be recorded as interop evidence under
   `docs/interop/`.

### Requirement 6 — Conformance against external tooling

**User Story:** As a reviewer, I want the 1.2 packets validated against independent
tooling where it exists, so that conformance is established, not asserted.

#### Acceptance Criteria
1. WHERE the DIFI Consortium reference definitions or Wireshark dissector support the 1.2
   packet classes THEN the toolkit's encoded packets SHALL validate/dissect cleanly against
   them, run headless as in the 1.1 harness.
2. WHERE external tooling does not yet cover a 1.2 packet THEN the toolkit SHALL provide
   committed golden packets with documented expected field values as the regression
   contract, and the gap SHALL be recorded honestly rather than implied as covered.
3. The toolkit SHALL provide golden packets (a few KB each) for the new 1.2 packet classes
   (`0x0005`, `0x0006`, and any `0x0002`/`0x0003` layout that differs from 1.1).

### Requirement 7 — Close known 1.1 implementation gaps

**User Story:** As a maintainer, I want the deferred gaps from the 1.1 work resolved as
part of this upgrade, so that the toolkit's conformance claims are complete and honest.

These gaps are carried over verbatim from `.kiro/specs/difi-iq-interop/` ("Known gaps"
and the "Implementation status" partials). Each is addressed here or explicitly re-deferred
with a recorded reason.

#### Acceptance Criteria
1. **(1.1 Req 4.3 — scaling vs DIFI reference-gain guidance.)** WHEN the sample-format
   scaling/reference-level convention is documented THEN it SHALL be reconciled against the
   DIFI MILCOM "Reference Gain and Scaling" guidance, and either confirmed consistent or
   adjusted, with the outcome recorded in the manifest and design.
2. **(1.1 Req 6.4 / DQ-4 — canonical dataset hosting.)** The toolkit SHALL either wire up a
   real externally-hosted canonical dataset (Zenodo or Release asset) referenced by
   SHA-256 from its manifest, OR record an explicit decision that the reproducible
   AFS-style waveform remains the committed reference, with the rationale documented (not
   left as a silent partial).
3. **(1.1 Task 13 — gr-difi end-to-end loss.)** WHERE gr-difi interop is claimed THEN it
   SHALL be verified by an end-to-end loss round-trip (reconciling gr-difi's own scaling),
   OR the limitation SHALL be documented precisely (what was and was not verified) rather
   than implied as full.
4. **(1.1 design DQ resolution.)** The design decisions resolved in practice during the 1.1
   work (DQ-1 Rust+C++, DQ-2 16-bit full-scale, DQ-3 version target) SHALL be reflected
   accurately in this feature's design, and the 1.1 `design.md` open-question wording SHALL
   be corrected to match reality.
5. WHERE a gap cannot be fully closed in this iteration THEN it SHALL be re-recorded as a
   known gap with a specific reason and a pointer to the increment that will address it —
   never silently dropped.

## Compatibility and scope

### Compatibility
- Additive only. The new 1.2 types live beside the existing 1.1 types in both
  implementations and reuse the existing transport, constants (OUI `0x6A621E`), big-endian
  32-bit-word packing, and fail-closed decoder posture.
- The Command-Packet foundation (CAM/CIF helpers, Controllee/Controller IDs, Buffer Size)
  built here is designed for reuse by the 1.3.0 link-establishment feature.

### Out of scope (this iteration)
- DIFI 1.3.0 link establishment (`0x0007`/`0x0008`/`0x0009` and the negotiation state
  machine) — a separate feature that builds on this one.
- Live ESTRACK hardware integration (the downstream HITL step).
- Full VITA 49.2 Command Packet generality beyond the DIFI 1.2 flow-control profile.
- Any reproduction of confidential specification text in the public repository.
