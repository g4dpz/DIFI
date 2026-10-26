# Design — DIFI 1.2 Data Plane Upgrade (Rust + C++)

## Overview

This document designs the upgrade of the two-implementation toolkit from DIFI 1.1 to the
DIFI 1.2.1 data plane, satisfying `requirements.md`. It adds the Timing Flow Control
packets (`0x0005`, `0x0006`), the additional standard-flow classes (`0x0002`, `0x0003`,
`0x0004`), and closes the known 1.1 gaps — all additively, without disturbing the shipped
1.1 behaviour.

The guiding principle is unchanged: DIFI is a *constrained schema* of VITA 49.2, so the
implementation stays small, explicit, and strict. The one architecturally significant
addition is the **Command-Packet foundation** — the CAM field, CIF0/CIF1 indicators,
Controllee/Controller IDs, and Buffer Size — which the Timing Flow Control packets need
and which the later 1.3.0 link-establishment packets will reuse. Building it cleanly here
is the main reason to do 1.2 before 1.3.0.

The authoritative Command-Packet field layout comes from the gitignored engineering
transcription (`work/spec-transcription-1.3.0.md`, §4.4.2 Table 4-22); this document and
the code paraphrase structure and behaviour only.

## Design decisions

- **DD-1 Additive, backward-compatible.** No existing 1.1 type, layout, constant value, or
  test changes. New classes are new constants and new encode/decode paths beside the 1.1
  ones. The 1.1 golden packets stay byte-identical (Req 4).
- **DD-2 Two independent implementations, kept in lockstep.** Rust and C++ each implement
  the new packets natively, validated against each other by byte-identical golden packets
  and cross-decode. Neither wraps the other (continues DQ-1).
- **DD-3 One shared Command-Packet core.** The CAM / CIF0 / CIF1 bit handling,
  Controllee/Controller IDs, Reference Point, Sample Rate, Timestamp Adjustment, and
  Buffer Size live in one place (a `command` module / namespace) that the two Timing Flow
  Control classes share and that the 1.3.0 control plane will build on. This avoids
  re-implementing the same bit-packing three times.
- **DD-4 Classes are data, not control flow.** Information/Packet class codes are named
  constants; the decoder dispatches on them via a table rather than scattered `if` chains,
  so adding `0x0002`/`0x0003`/`0x0004`/`0x0005`/`0x0006` is a table extension, not a
  rewrite of the dispatch.
- **DD-5 Fail-closed decode for the new packets too.** Command packets and flow-control
  fields are bounds-checked against the received length before use; the decoder never
  best-effort mis-parses (same posture as the 1.1 decoder).
- **DD-6 Close gaps, or re-defer with a reason.** Each 1.1 known gap (Req 7) is either
  resolved here or explicitly re-recorded with a specific reason — never silently dropped.

## What 1.2 changes, concretely (verified against Table 4-1 and Table 3-1)

The authoritative Packet-Type/Packet-Class table (Table 4-1) and Information-Class table
(Table 3-1) give the exact additions. **Packet Class and Information Class are different
fields that share numeric values** — the earlier draft of this spec conflated them; this
is the corrected account.

**Packet Classes (Table 4-1) — the actual packet layouts:**

| Packet Type | Packet Class | Meaning | Status for us |
|-------------|--------------|---------|---------------|
| 0x1 Data | `0x0000` | Standard Flow Signal Data | 1.1 — implemented |
| 0x1 Data | `0x0002` | **Sample Count Signal Data** | NEW — same layout as 0x0000, TSF = sample count |
| 0x4 Context | `0x0001` | Standard Flow Signal Context | 1.1 — implemented |
| 0x4 Context | `0x0003` | **Sample Count Signal Context** | NEW — same layout as 0x0001, TSF = sample count |
| 0x4 Context | `0x0004` | Version Flow Signal Context | 1.1 — already implemented (our `version_context`) |
| 0x6 Command | `0x0005` | Sample Count Timing Flow Control | NEW — the real new work |
| 0x6 Command | `0x0006` | Real-Time TSF Timing Flow Control | NEW — the real new work |

**Information Classes (Table 3-1) — groupings, NOT packet layouts.** An Information Class
names the set of Packet Streams that make up an Information Stream; it does not define a
packet's bits:

| Info Class | Comprises | Note |
|-----------|-----------|------|
| `0x0000` | Signal Data + Signal Context | 1.1 "Basic Data Plane" |
| `0x0004` | Signal Data + Signal Context | 1.2.1 — a second "Basic Data Plane" grouping |
| `0x0002` | Signal Data + Signal Context + Timing Flow Control | 1.2.0 — "Data Plane plus Flow Control" |
| `0x0003` | as `0x0002`, real-time-TSF variant | 1.2.0 |
| `0x0001` | Version Flow Signal Context packet | 1.1 "Version Flow" |

**So the concrete new packet work is small and sharply defined:**
1. The two Timing Flow Control Command packets (`0x0005`/`0x0006`) — genuinely new, built
   on the shared `command` core. This is the bulk of the work.
2. The two sample-count twins (`0x0002` Signal Data, `0x0003` Signal Context) — same field
   layout as `0x0000`/`0x0001`, differing only in class code and TSF mode (which the
   toolkit already models in its `tsf` constants). Implemented as a thin variant of the
   existing Signal Data / Signal Context codecs, not new layouts.
3. Recognition/routing of Information Classes `0x0002`/`0x0003`/`0x0004` — constants and
   dispatch only; no new packet bits. `0x0004` as a *Packet* Class is the Version Context
   we already ship.

## Wire format — Timing Flow Control Command Packet (as transcribed)

All words are 32-bit big-endian, MSB = bit 31. Base layout (both `0x0005` and `0x0006`):

```
Word 1   Header: [31-28] Packet Type = 0x6 (Command Packet w/ stream ID)
                 [27] Class-ID Indicator  [26-24] Control/Ack indicators (0 for a
                 non-cancellation command)  [23-22] TSI  [21-20] TSF
                 [19-16] SeqNum (mod-16)  [15-0] Packet Size
Word 2   Stream Identifier
Word 3   [31-24] 0 (incl zero pad)  [23-0] OUI 0x6A621E
Word 4   [31-16] Information Class  [15-0] Packet Class (0x0005 or 0x0006)
Word 5   Integer-seconds Timestamp
Word 6-7 Fractional-seconds Timestamp  (sample count for 0x0005; real-time ps for 0x0006)
Word 8   CIF0  (field-presence bitmap: Reference Point, Sample Rate, Timestamp Adj, CIF1)
Word 9   Message ID
Word 10  Controllee ID   (default 0x00000000 if unused)
Word 11  Controller ID   (default 0x00000000 if unused)
Word 12  CIF1  (indicates Buffer Size field present, among others)
Word 13  CIF1 (cont.)
Word 14  Reference Point  (default 0x00000064)
Word 15-16  Sample Rate (64-bit)
Word 17-18  Timestamp Adjustment (femtoseconds, 64-bit)
Word 19-20  Buffer Size (64-bit)
Word 21  [..] Level  [3] Overflow (OF)  [2] Near-Full (NF)  [1] Near-Empty (NE)
                      [0] Underflow (UF)   — Buffer Status flags
```

- **Packet Class `0x0005`** = Sample Count Timing Flow Control; TSF carries a sample count.
- **Packet Class `0x0006`** = Real-Time TSF Timing Flow Control; TSF carries real-time
  picoseconds.
- The CAM field is not shown in the base word table above; for flow control the VITA 49.2
  control/acknowledge semantics default to "execute", with Controllee/Controller ID present
  bits set when those IDs are used. The shared `command` module owns the exact CAM bit
  layout (the same one the 1.3.0 query/ack/status packets reuse).

## Components

Both implementations mirror this structure. New Rust modules in `rust/src/`; new content
in the C++ header `cpp/include/difi/difi.hpp`.

### 1. Extended constants (`constants`)
- Add Information-Class codes `0x0002` (Data Plane plus Flow Control), `0x0003` (its
  real-time-TSF variant), and `0x0004` (second Basic Data Plane grouping) to
  `information_class`, beside the existing `0x0000`/`0x0001`.
- Add Packet-Class codes `0x0002` (Sample Count Signal Data), `0x0003` (Sample Count
  Signal Context), `0x0005`/`0x0006` (Timing Flow Control) to `packet_class`, beside the
  existing `0x0000`/`0x0001`/`0x0004`. (Packet Class `0x0004` Version Context already
  exists — unchanged.)
- Add the Command-Packet VRT type `0x6` to `packet_type` (type `0x7` extension is for the
  separate 1.3.0 feature).
- Add Buffer Status flag bit positions (OF/NF/NE/UF) and the Reference Point default.
- Keep Information-Class and Packet-Class constants in clearly separate namespaces/modules
  so their shared numeric values are never confused (they live in different packet fields).
  None of the existing 1.1 values change.
- _Files: `rust/src/constants.rs`, `cpp/include/difi/difi.hpp` (`constants`)._

### 2. Shared Command-Packet core (`command`)
- CAM field read/write (Controllee/Controller ID present bits, action/acknowledge mode),
  CIF0/CIF1 field-presence bit helpers, and the common context words (Message ID,
  Controllee ID, Controller ID, Reference Point, Sample Rate, Timestamp Adjustment, Buffer
  Size). This is the piece the 1.3.0 control plane will import.
- _Files: `rust/src/command.rs`, `cpp/include/difi/difi.hpp` (`difi::command`)._

### 3. Timing Flow Control packet model (`flowcontrol`)
- `TimingFlowControlPacket` (or two thin typed wrappers for `0x0005`/`0x0006`) with
  `pack()`/`unpack()` built on the `command` core; carries the Buffer Status flags and the
  class-specific TSF interpretation.
- _Files: `rust/src/flowcontrol.rs`, `cpp/include/difi/difi.hpp` (`difi::flowcontrol`)._

### 4. Sample-count twins + decoder dispatch extension (`decoder`, `packet`)
- Sample-count Data/Context (`0x0002`/`0x0003`): reuse the existing Signal Data / Signal
  Context field codecs with the class code and TSF mode set to sample-count — a thin
  variant, not a new layout. Encode support mirrors this.
- Extend the decoder's dispatch table (DD-4) to recognize the new Packet Classes
  (`0x0002`/`0x0003`/`0x0005`/`0x0006`) and the new Information Classes
  (`0x0002`/`0x0003`/`0x0004`), routing each by Packet Class to the right codec, with the
  same fail-closed validation. 1.1 packets keep their exact existing path (Req 4.4), and
  Packet Class `0x0004` continues to route to the existing Version Context path.
- _Files: `rust/src/decoder.rs`, `rust/src/encoder.rs`, `rust/src/packet.rs`, C++ header._

### 5. Golden packets + conformance
- Add golden packets for `0x0005` and `0x0006` (and any `0x0002`/`0x0003` layout that
  differs from 1.1) under `tests/golden/`, each with a documented field-value manifest and
  SHA-256, generated byte-identically by both implementations.
- Extend `tests/conformance/` to validate the new packets against the DIFI reference
  definitions / Wireshark dissector where 1.2.1 is supported; record gaps honestly
  otherwise.

### 6. Gap-closure work (Req 7) — not new packet code, but part of this feature
- **Scaling vs reference-gain guidance (Req 7.1):** reconcile the documented
  `ScalingConvention` (16-bit, full-scale = 2^(n-1)-1) against the DIFI MILCOM Reference
  Gain and Scaling guidance; confirm or adjust; record the outcome in the manifest and in
  this design. _Files: `rust/src/mapper.rs` (if adjusted), `docs/`, manifest._
- **Canonical dataset hosting (Req 7.2):** make an explicit decision — host a real vector
  externally by SHA-256, or formally keep the reproducible AFS-style waveform as the
  committed reference with rationale. Record it; do not leave it a silent partial.
- **gr-difi end-to-end loss (Req 7.3):** either add an end-to-end loss round-trip that
  reconciles gr-difi's own scaling, or document precisely what is and is not verified.
- **1.1 design doc correction (Req 7.4):** update `.kiro/specs/difi-iq-interop/design.md`
  so DQ-1/DQ-2/DQ-3 read as resolved (they are, in practice), matching the tasks.md
  "Known gaps" note that already flags the discrepancy.

## Data flow — a flow-controlled stream (Req 5.3)

1. **Source** (e.g. C++ `difi-tool`) emits Signal Context + Signal Data as today (1.1 path,
   unchanged).
2. **Sink** (e.g. Rust) tracks its buffer fill level and periodically emits a Timing Flow
   Control packet (`0x0006`, real-time TSF) carrying Buffer Size and the OF/NF/NE/UF flags.
3. **Source** decodes the flow-control packet and paces its stream accordingly (e.g. slows
   on Near-Full, resumes on Near-Empty). The pacing policy is the driver's; the toolkit's
   job is correct encode/decode of the flow-control state.
4. The exchange runs over the existing UDP transport, and the packets + decoded fields are
   recorded as interop evidence under `docs/interop/`.

## Error handling
- **Decode:** fail-closed — reject wrong OUI, unrecognized class, size/length mismatch, or
  a field offset that overruns the buffer, each with a typed, field-level error.
- **Encode:** validate up front — Packet Size must equal the emitted word count, CAM/CIF
  bits must be consistent with the fields present — and refuse to emit otherwise.

## Testing strategy
- **Unit:** pack/unpack round-trip for `0x0005` and `0x0006` incl. Buffer Status flags and
  the class-specific TSF interpretation; the shared `command` CAM/CIF helpers tested
  directly; fail-closed rejection of malformed command buffers.
- **Regression (Req 4):** the full existing 1.1 suite runs unchanged and green; the 1.1
  golden packets assert byte-identical; a test confirms a 1.1 packet still decodes via the
  extended decoder exactly as before.
- **Golden:** committed per-class packets with documented fields; both implementations
  assert a byte match (as the 1.1 `golden_match` ctest does).
- **Interop:** Rust↔C++ cross-decode of each new packet; a scripted flow-controlled UDP
  exchange both directions, recorded under `docs/interop/` (extend `tests/interop.sh`).
- **Conformance:** validate against external tooling where 1.2.1 is supported; otherwise
  record the gap and rely on golden packets.
- **Gap closure (Req 7):** the reconciliation/decision artifacts exist and are referenced;
  the 1.1 design doc is corrected; re-deferred items carry a specific reason.
- **Property/fuzz (stretch):** random valid flow-control configs round-trip bit-exact;
  malformed command buffers never panic/crash the decoder; C++ clean under ASan+UBSan.

## Security / robustness
- Command/flow-control packets are untrusted network input: bounds-check every field and
  offset against the received length before use; treat all content as untrusted data
  (same posture carried from the 1.1 decoder).

## Compatibility and alignment
- **Additive:** no change to 1.1 layouts, API semantics, or passing tests; shared
  infrastructure reused, not forked (Req 4).
- **Foundation for 1.3.0:** the `command` core (CAM/CIF, IDs, Buffer Size) is the base the
  1.3.0 link-establishment packets extend; after this lands, the `difi-1.3.0-exploration`
  branch rebases onto it rather than rebuilding the foundation.
- **Confidentiality:** the public repo describes structure and behaviour in our own words;
  the confidential transcription and PDF stay in gitignored `work/`.
- **Community hub:** the Rust↔C++ 1.2 interop evidence is structured for contribution to
  the esa/CCSDS community prototyping hub, as the 1.1 evidence is.
