# Design — DIFI I/Q Interoperability Toolkit

## Overview

This document designs a reference DIFI I/Q exchange layer satisfying the requirements in
`requirements.md`. The scope of this iteration is a **DIFI 1.1** encoder/decoder, a UDP
transport, a float↔integer sample-format mapping, reproducible signal generators, and
conformance validation against the DIFI Consortium tooling.

As built, this is delivered as **two independent implementations — Rust and C++** (see
DQ-1 below) that interoperate, rather than the single Python package first proposed.

The guiding principle mirrors how the standard itself works: DIFI is a *constrained
schema* of VITA 49.2, so the implementation should be small, explicit, and strict —
rejecting anything outside the DIFI profile rather than trying to be a general VRT stack.

See `docs/reference/DIFI-VITA49-PRIMER.md` for the standard's field-level details.

## Design decisions (and their resolution)

These started as open questions deferred to the team. Their current resolution is
recorded here; see `tasks.md` and `IMPLEMENTATIONS.md` for the implementing work.

- **DQ-1 Implementation language — RESOLVED: two independent implementations, Rust +
  C++.** The original proposal was a single **Python** package (matches the certification
  tooling, lowest barrier). We instead built **two independent implementations** (a Rust
  library crate and a C++17 header-only library). Rationale: two independent
  implementations exchanging I/Q is a stronger interop demonstration than one reference,
  and gives a compiled core for the eventual HITL/throughput work. They validate against
  each other, the DIFI Consortium reference, and gr-difi. A Python reference could still
  be added later as a third implementation checked against the same golden packets.
- **DQ-2 Sample bit depth + scaling — RESOLVED (with a caveat): 16-bit signed,
  full-scale = 2^(n-1)-1.** `float→int = round(clamp(x/full_scale, -1, 1) · FS)`,
  round-half-away-from-zero, saturating; `full_scale` recorded in the manifest.
  **Caveat (Req 4.3):** the convention is documented and pinned but **not yet reconciled
  against the DIFI MILCOM reference-gain/scaling guidance** — open follow-up.
- **DQ-3 DIFI version — RESOLVED for this feature: 1.1.** This `difi-iq-interop` feature
  implements the DIFI 1.1 data plane; the packet-type/class handling deliberately leaves
  room for later versions. The version target of the *toolkit as a whole* has since
  advanced beyond 1.1: ESA's ESTRACK will use **DIFI 1.3.0**, and two follow-on features
  now carry the toolkit there — `difi-1.2-dataplane` (the 1.2 flow-control / sample-count
  data plane and the shared Command-Packet core) and `difi-link-establishment-1.3.0` (the
  1.3.0 control plane). Build order: 1.1 (this feature, shipped) → 1.2 → 1.3.0 = full DIFI
  1.3.0. So "1.1 only" describes this feature's scope, not a decision to stop at 1.1.
- **DQ-4 Canonical dataset hosting — OPEN.** The manifest records a SHA-256 so large data
  can be hosted externally (Zenodo vs Release assets), but no real canonical dataset is
  wired up yet; the committed reference is a reproducible LSIS-AFS-*style* waveform, not
  a real LSIS-AFS vector. Policy question, not code.

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
- `SignalDataPacket` — header (packet type, class-id bit, TSM, TSI, TSF, 4-bit seq num,
  packet size), Stream ID, OUI, Info/Packet class codes, integer + fractional (psec)
  timestamps, I/Q payload.
- `SignalContextPacket` — RF reference frequency, bandwidth, sample rate, gain, data
  format (bit depth / packing), timestamps.
- `VersionContextPacket` — sender/spec version fields.

Each packet type has a `pack()` → bytes and an `unpack(bytes)` constructor.
Bit/field packing is **big-endian**, 32-bit-word aligned, per the primer. Constants
(OUI `0x6A621E`, header = 7 words, class codes) live in one `constants` module.

**As built (conformance-driven, see task 10):** the context and version packets are NOT
loose field bags — they are the authoritative DIFI layouts. Signal Context is a fixed
**27-word** packet (VRT type `0x4`, TSM=1, `cif0`, `refPoint=0x64`, fixed-point 2^20/2^7
scaled fields, `dataPacketFormat` carrying bit depth). Version Context is a fixed
**11-word** packet (VRT type **`0x5`**, `cif1`, `v49SpecVersion`). These match the DIFI
Consortium reference `construct` definitions exactly.

### 2. DIFI Encoder
Builds valid packets from a packet model + stream config. Responsibilities:
- Compute Packet Size (words) and set it correctly.
- Maintain and increment the mod-16 sequence number across a data stream.
- Enforce the whole-4-byte-word constraint for the chosen samples-per-packet × bit depth;
  reject invalid configs (Req 2.4).
- Enforce the ≤~9000-octet UDP payload limit; reject or segment per config (Req 2.5).
  *As built: both — `encode_data` rejects an oversized single packet, and
  `encode_data_segmented` splits a large buffer into word-aligned packets with running
  sequence numbers (cap from `max_samples_per_packet`).*

### 3. DIFI Decoder + Validator
Parses bytes into the packet model, failing closed on anything outside the DIFI profile:
- Reject non-VITA OUI (Req 1.4); reject Packet-Size/length mismatch (Req 1.5).
- Correctly unpack 4–16 bit signed I/Q, including sub-byte packing with no zero padding
  (Req 1.6).
- Surface precise diagnostics (which field, expected vs actual) rather than mis-parsing.

### 4. Sample Mapper (float ↔ DIFI integer)
Deterministic quantization with an explicit, documented scaling/reference-level
convention (DQ-2). Provides `float_to_difi(iq, convention)` and the inverse (the
convention carries bit depth + full-scale), and a `measure_implementation_loss()` helper
that reports round-trip error (Req 4.2) — never assumes lossless. The convention is
written into the signal manifest. *Caveat: not yet reconciled against the DIFI MILCOM
reference-gain guidance (Req 4.3).*

### 5. Transport
- **UDP** sender/receiver (the DIFI-standard transport, Req 5.1). *As built.*
- Optional **TCP** mode for lossless development exchange, clearly labelled non-standard
  (Req 5.2). *As built: `TcpSender`/`TcpReceiver`, length-framed (4-byte big-endian
  prefix per packet, since TCP has no datagram boundaries), with Stream-ID filtering.
  Explicitly non-DIFI; will not interoperate with a standard UDP endpoint.*
- Receiver filters by **Stream ID** so multiple streams can share a port (Req 5.3).
  *As built.*
- Transport is decoupled from the packet model (encode/decode never touch sockets), so
  packets can be unit-tested and golden-compared without the network.

### 6. Generators + Manifest writer
- Signal generators reproduce reference signals deterministically from committed params.
- Manifest writer emits SHA-256, size, sample format, DIFI context params, and the exact
  regeneration command (Req 6.1–6.2). Enforces the artifact policy by construction:
  generators write large output to a gitignored working dir, never into tracked paths.

### 7. Conformance harness
Validation against the external DIFI tooling. *As built, this uses the DIFI Consortium
reference `construct` packet definitions (fetched on demand, pinned, not vendored)
rather than driving the full `certify_*.py` scripts — the definitions + their
`validate()` methods are the authoritative wire contract and the substantive part of
what the certifier checks.*
- Our packets parse + validate clean against the reference definitions
  (`certify.py`, Req 3.2).
- Reference-built packets decode in our tools; both directions
  (`cross_decode.py`, Req 3.3, 7.2).
- DIFI Wireshark dissector check (Req 3.1): **done** — `wireshark_check.py` runs the
  Consortium `difi-dissector.lua` headless via `tshark` over a pcap of our packets and
  asserts they dissect as DIFI with the right fields.
- Golden packets committed as fixtures (Req 3.4).
- Also: GNU Radio interop (socket_pdu Route 1, and gr-difi's own blocks Route 2) —
  see `docs/interop/`.

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
- **Conformance:** our packets validate against the DIFI Consortium reference `construct`
  definitions (Req 3.2, 3.3) and dissect cleanly in the DIFI Wireshark dissector via
  `tshark` (Req 3.1).
- **Interop:** the two implementations exchange I/Q (byte-identical generation,
  cross-decode, live UDP, export/import); cross-decode with the reference defs, GNU Radio
  (socket_pdu), and gr-difi's own blocks (Req 7.2) — all recorded under `docs/interop/`.
- **Property/fuzz (stretch, done):** random valid configs round-trip; ~20k malformed
  buffers never crash the decoder; C++ fuzz clean under ASan+UBSan.
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
