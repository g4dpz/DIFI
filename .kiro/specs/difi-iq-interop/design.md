# Design — DIFI I/Q Interoperability Toolkit

## Overview

This document designs a reference DIFI I/Q exchange layer satisfying the requirements in
`requirements.md`. The scope of this iteration is a **DIFI 1.1** encoder/decoder, a UDP
transport, a float↔integer sample-format mapping, reproducible signal generators, and
conformance validation against the DIFI Consortium tooling.

The guiding principle mirrors how the standard itself works: DIFI is a *constrained
schema* of VITA 49.2, so the implementation should be small, explicit, and strict —
rejecting anything outside the DIFI profile rather than trying to be a general VRT stack.

See `docs/reference/DIFI-VITA49-PRIMER.md` for the standard's field-level details.

## Design decisions and open questions

A few decisions are deliberately deferred to the team (recorded here, not silently
chosen), because they have external constraints:

- **DQ-1 Implementation language.** Candidates: **Python** (matches the DIFI
  certification tooling, gr-difi bindings, fastest to interop, lowest barrier for the
  community) vs a compiled core (C++/Rust) for throughput/HITL. *Proposed:* start in
  **Python** for the reference encoder/decoder and conformance work; revisit a compiled
  core only if HITL throughput demands it. Rationale: the first job is wire-correctness
  and interop, not line rate.
- **DQ-2 Sample bit depth + scaling convention** (Requirement 4). DIFI allows 4–16 bit
  signed integers; the competition signals are float32. The scaling/reference-level
  convention must be pinned against the DIFI reference-gain/scaling guidance. *Proposed
  default:* 16-bit signed, full-scale mapping documented in the manifest; confirm with the
  team and against ESTRACK expectations.
- **DQ-3 DIFI version.** 1.1 now; 1.2.1 (flow control, command packets) later. Design
  keeps packet-type handling open for extension (see Packet Registry below).
- **DQ-4 Canonical dataset hosting.** Zenodo (DOIs) vs Release assets — policy question,
  not code; see artifact policy.

These map to the "Open questions" in the primer and the proposal email.

## Architecture

```
            +-------------------------------------------------------+
            |                    difi-interop toolkit               |
            |                                                       |
  I/Q (float|  +-----------+    +-----------------+   +-----------+ |
  or int)   -->| Sample    |--->| DIFI Encoder    |-->| Transport | |---> UDP out
            |  | Mapper    |    | (data/context/  |   | (UDP;     | |
  I/Q      <---| (float<-> |<---| version packets)|<--| opt TCP)  | |<--- UDP in
            |  |  integer) |    +-----------------+   +-----------+ |
            |  +-----------+    +-----------------+                 |
            |                   | DIFI Decoder    |                 |
            |                   | + validator     |                 |
            |                   +-----------------+                 |
            |                                                       |
            |  +-------------+   +------------------+               |
            |  | Generators  |-->| Manifest writer  |  (repro data) |
            |  +-------------+   +------------------+               |
            +-------------------------------------------------------+
                         |                 |
                  golden packets    conformance harness
                  (committed KB)    (gr-difi / certify_*/ Wireshark)
```

## Components

### 1. Packet model
Typed representations of the three DIFI 1.1 packets:
- `SignalDataPacket` — header (packet type, class-id bit, TSI, TSF, 4-bit seq num,
  packet size), Stream ID, OUI, Info/Packet class codes, integer + fractional (psec)
  timestamps, I/Q payload.
- `SignalContextPacket` — RF reference frequency, bandwidth, sample rate, gain, data
  format (bit depth / packing), timestamps.
- `VersionContextPacket` — sender version, DIFI spec version.

Each packet type has a `pack() -> bytes` and `classmethod unpack(bytes)`.
Bit/field packing is **big-endian**, 32-bit-word aligned, per the primer. Constants
(OUI `0x6A621E`, header = 7 words, class codes) live in one `constants` module.

### 2. DIFI Encoder
Builds valid packets from a packet model + stream config. Responsibilities:
- Compute Packet Size (words) and set it correctly.
- Maintain and increment the mod-16 sequence number across a data stream.
- Enforce the whole-4-byte-word constraint for the chosen samples-per-packet × bit depth;
  reject invalid configs (Req 2.4).
- Enforce the ≤~9000-octet UDP payload limit; reject or segment per config (Req 2.5).

### 3. DIFI Decoder + Validator
Parses bytes into the packet model, failing closed on anything outside the DIFI profile:
- Reject non-VITA OUI (Req 1.4); reject Packet-Size/length mismatch (Req 1.5).
- Correctly unpack 4–16 bit signed I/Q, including sub-byte packing with no zero padding
  (Req 1.6).
- Surface precise diagnostics (which field, expected vs actual) rather than mis-parsing.

### 4. Sample Mapper (float ↔ DIFI integer)
Deterministic quantization with an explicit, documented scaling/reference-level
convention (DQ-2). Provides `float_to_difi(iq, bit_depth, scale)` and the inverse, and a
`measure_implementation_loss()` helper that reports round-trip error (Req 4.2) — never
assumes lossless. The convention is written into the signal manifest.

### 5. Transport
- **UDP** sender/receiver (the DIFI-standard transport, Req 5.1).
- Optional **TCP** mode for lossless development exchange, clearly labelled non-standard
  (Req 5.2).
- Receiver filters by **Stream ID** so multiple streams can share a port (Req 5.3).
- Transport is decoupled from the packet model (encode/decode never touch sockets), so
  packets can be unit-tested and golden-compared without the network.

### 6. Generators + Manifest writer
- Signal generators reproduce reference signals deterministically from committed params.
- Manifest writer emits SHA-256, size, sample format, DIFI context params, and the exact
  regeneration command (Req 6.1–6.2). Enforces the artifact policy by construction:
  generators write large output to a gitignored working dir, never into tracked paths.

### 7. Conformance harness
Scripts that drive the external DIFI tooling:
- Encode → parse with DIFI `certify_source.py` (Req 3.2).
- `certify_sink.py` → decode with our decoder (Req 3.3).
- Produce a capture for the DIFI Wireshark dissector (Req 3.1).
- Golden packets committed as fixtures (Req 3.4).

## Data flow — export an existing LSIS-AFS signal over DIFI (Req 7.1)
1. Load the float32 reference I/Q (regenerated from its generator, not a committed blob).
2. Sample Mapper → DIFI integer at the configured bit depth/scale.
3. Encoder → Signal Context packet (describing the stream) + a sequence of Signal Data
   packets (with running seq numbers).
4. Transport → UDP to the receiver (or another implementation such as gr-difi).
5. Receiver → Decoder → Sample Mapper (inverse) → float32.
6. Compare recovered vs original within documented quantization loss; record as interop
   evidence.

## Error handling
- Decoder is **fail-closed**: any profile violation (OUI, size, class code, bit-depth
  packing) raises a typed error with field-level detail; it never returns a best-effort
  mis-parse.
- Encoder validates configuration up front (word alignment, payload size) and refuses to
  emit malformed packets.
- Transport surfaces loss/out-of-order via the sequence number; it does not hide it.

## Testing strategy
- **Unit:** pack/unpack round-trip for all three packet types; bit-exact I/Q for each
  supported bit depth incl. sub-byte packing; config-validation rejections.
- **Golden:** committed KB-scale packets with documented field values as regression
  fixtures.
- **Conformance:** against DIFI `certify_*` and the Wireshark dissector (Req 3).
- **Interop:** cross-decode with gr-difi both directions where available (Req 7.2);
  record evidence.
- **Property/fuzz (stretch):** random valid configs round-trip; malformed input never
  crashes the decoder, only raises typed errors.
- Sample-mapper loss is **measured and reported**, not assumed.

## Security / robustness
- The decoder parses untrusted network input: bounds-check every field against the
  received length before use; cap allocation by the validated Packet Size; treat all
  packet content as untrusted data.

## Alignment with project rules
- **Artifact policy:** generators + manifests + small golden packets only; large data
  external by checksum.
- **Community hub:** this is an independent implementation that the
  esa/CCSDS-Community-Prototyping hub can link to; interop evidence is structured for
  contribution back.
