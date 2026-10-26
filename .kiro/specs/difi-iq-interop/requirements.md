# Requirements — DIFI I/Q Interoperability Toolkit

## Introduction

This feature delivers a reference implementation of the **DIFI** (Digital IF
Interoperability) I/Q exchange layer: the ability to encode and decode DIFI packets
(IEEE-ISTO Std 4900-2021, a constrained VITA 49.2 profile over UDP/IP), so that
independent CCSDS reference implementations — and, in a later step, ESA ESTRACK
hardware — can exchange digitized I/Q over one standard interface.

The initial target is **DIFI version 1.1** (Signal Data, Signal Context, Version
Context packets), with the design leaving room for 1.2.1 (flow control, command
packets). The toolkit must be correct to the wire-level standard, validated against the
DIFI Consortium's own certification tooling, and must respect the project artifact
policy (no large raw I/Q in Git).

See `docs/reference/DIFI-VITA49-PRIMER.md` for the standard's details and sources.

## Glossary

- **DIFI** — Digital IF Interoperability standard (IEEE-ISTO Std 4900-2021).
- **VRT / VITA 49.2** — the parent packet standard DIFI profiles.
- **Signal Data packet** — carries I/Q samples + precise timestamp.
- **Signal Context packet** — carries stream metadata (RF freq, bandwidth, sample rate,
  gain, data/sample format).
- **Version Context packet** — carries sender and DIFI-spec version.
- **I/Q** — complex baseband samples (In-phase, Quadrature).

## Requirements

### Requirement 1 — Decode DIFI packets

**User Story:** As an implementer, I want to decode DIFI packets received over UDP, so
that my application can consume I/Q and metadata produced by any DIFI-compliant source.

#### Acceptance Criteria
1. WHEN a DIFI 1.1 Signal Data packet is received THEN the toolkit SHALL decode the
   header fields (packet type, class-id bit, TSI, TSF, sequence number, packet size),
   Stream ID, OUI, Information/Packet Class codes, integer- and fractional-seconds
   timestamps, and the I/Q payload.
2. WHEN a Signal Context packet is received THEN the toolkit SHALL decode the stream
   metadata including RF reference frequency, bandwidth, sample rate, gain, and the
   sample data format (bit depth and packing).
3. WHEN a Version Context packet is received THEN the toolkit SHALL decode the sender
   version and DIFI spec version fields.
4. WHEN a received packet's OUI is not the VITA OUI (`0x6A621E`) THEN the toolkit SHALL
   reject it as non-DIFI and report the reason.
5. WHEN a packet's Packet Size field does not match the actual received length THEN the
   toolkit SHALL reject it and report the mismatch rather than mis-parsing.
6. WHEN the Signal Data payload uses a signed-integer bit depth between 4 and 16 THEN the
   toolkit SHALL correctly unpack I and Q samples, including sub-byte-packed depths with
   no zero padding.

### Requirement 2 — Encode DIFI packets

**User Story:** As an implementer, I want to encode I/Q and metadata into DIFI packets,
so that any DIFI-compliant receiver can consume my signal.

#### Acceptance Criteria
1. WHEN I/Q samples and stream parameters are provided THEN the toolkit SHALL produce a
   valid DIFI 1.1 Signal Data packet with correct header, OUI, class codes, timestamps,
   Packet Size, and payload.
2. WHEN stream metadata is provided THEN the toolkit SHALL produce a valid Signal Context
   packet describing it.
3. WHEN emitting a stream THEN the toolkit SHALL maintain the 4-bit sequence number
   (mod-16) across successive Signal Data packets.
4. WHEN a chosen samples-per-packet and bit depth would not yield a whole number of
   4-byte words THEN the toolkit SHALL reject the configuration with a clear error rather
   than emit a malformed packet.
5. WHEN the UDP payload size would exceed the DIFI maximum (~9000 octets) THEN the
   toolkit SHALL reject or segment per configuration, never silently truncate.
6. WHEN a Signal Data packet is encoded and then decoded (round trip) THEN the recovered
   fields and I/Q samples SHALL match the inputs exactly (bit-exact for the chosen
   integer format).

### Requirement 3 — Wire-level conformance and certification

**User Story:** As a reviewer, I want the toolkit's output validated against the DIFI
Consortium's own tooling, so that conformance is independently established, not asserted.

#### Acceptance Criteria
1. WHEN the toolkit encodes a packet THEN the packet SHALL parse cleanly in the DIFI
   Consortium Wireshark dissector with all fields interpreted correctly.
2. WHEN the toolkit encodes a packet THEN it SHALL be accepted by the DIFI certification
   reference parser (`certify_source.py`) without error.
3. WHEN the DIFI certification reference generator (`certify_sink.py`) produces a packet
   THEN the toolkit SHALL decode it and recover the expected fields and samples.
4. The toolkit SHALL provide a small set of committed **golden packets** (a few KB each)
   with documented expected field values, usable as regression fixtures.

### Requirement 4 — Sample-format mapping (float ↔ DIFI integer)

**User Story:** As an implementer with float32 reference signals (as from the
competition), I want a defined mapping to DIFI integer samples, so that existing signals
can be exchanged over DIFI without ambiguity.

#### Acceptance Criteria
1. WHEN a float32 I/Q signal is converted to a DIFI integer format THEN the toolkit SHALL
   apply a documented, deterministic quantization with an explicit scaling/reference-level
   convention and the selected bit depth.
2. WHEN a DIFI integer signal is converted back to float THEN the toolkit SHALL apply the
   inverse scaling, and the round-trip implementation loss SHALL be measured and reported
   (not assumed zero).
3. The chosen scaling/reference-level convention SHALL be documented against the DIFI
   reference-gain/scaling guidance and recorded in the signal manifest.

### Requirement 5 — Transport

**User Story:** As a developer, I want to send and receive DIFI over the network, so that
I can exercise interop between two processes or machines.

#### Acceptance Criteria
1. WHEN streaming is requested THEN the toolkit SHALL send and receive DIFI packets over
   **UDP/IP** per the standard.
2. WHERE lossless development/file-like exchange is needed THEN the toolkit MAY support a
   TCP transport option, clearly marked as a development convenience, not DIFI-standard
   transport.
3. WHEN multiple streams share a port THEN the toolkit SHALL filter received packets by
   Stream ID.

### Requirement 6 — Reproducible signals and artifact discipline

**User Story:** As a maintainer, I want large I/Q reproducible rather than stored, so the
repository stays small as the ecosystem grows.

#### Acceptance Criteria
1. The toolkit SHALL provide signal **generators** that deterministically reproduce
   reference signals from committed parameters.
2. Each generated reference signal SHALL have a **manifest** recording SHA-256, byte size,
   sample format, DIFI context parameters, and the exact regeneration command.
3. The repository SHALL NOT contain raw I/Q files larger than ~1 MB in Git history
   (enforced by `.gitignore`; see `.kiro/steering/artifact-policy.md`).
4. WHERE a large canonical dataset must be preserved as-is THEN it SHALL be hosted
   externally (Zenodo or Release assets) and referenced by SHA-256 from its manifest.

### Requirement 7 — Interoperability with existing implementations

**User Story:** As the community, we want to prove DIFI exchange works across independent
implementations, not just internally.

#### Acceptance Criteria
1. WHEN a reference signal (e.g. an LSIS-AFS signal) is exported to DIFI and re-imported
   THEN the recovered signal SHALL match within the documented quantization loss.
2. WHERE a second independent DIFI implementation is available (e.g. gr-difi) THEN the
   toolkit SHALL demonstrate cross-decode in both directions and record the result.
3. Interop results SHALL be recorded as evidence (commands, fields, pass/fail), suitable
   for contribution to the CCSDS community prototyping hub.

## Out of scope (this iteration)
- DIFI 1.2.1 flow-control and command packets (design must not preclude them, but they
  are a later increment).
- Live ESTRACK hardware integration (the HITL second step; this toolkit enables it).
- Full VITA 49.2 generality beyond the DIFI profile.
