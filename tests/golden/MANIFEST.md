# Golden DIFI 1.1 Packets — Shared Conformance Oracle

These three packets are the **shared wire-format oracle** for every independent
implementation in this toolkit (Rust, C++). They are small (≤108 bytes each) and are
committed deliberately (the artifact policy allowlists `tests/golden/`). Any
implementation claiming DIFI 1.1 conformance here MUST:

1. **Decode** each packet and recover the field values documented below.
2. **Encode** the same inputs and produce byte-identical output (matching the SHA-256).
3. **Pass** the DIFI Consortium reference validation (`tests/conformance/certify.py`).

These packets are validated against the DIFI Consortium's own `construct` packet
definitions — they are not merely self-consistent. The layout follows
`difi_*_v1_1.py` from the certification repo (see `tests/conformance/README.md`).

## Shared encoding conventions

- DIFI **1.1**, VITA OUI `0x6A621E`, 7-word (28-byte) header, **big-endian**.
- Header word 0: pktType(4) classId(1) reserved(2) **TSM(1)** TSI(2) TSF(2) seqNum(4)
  pktSize(16). TSI = UTC (`0b01`), TSF = real-time picoseconds (`0b10`).
- Signal Data: TSM=0, pktType `0x1`, ICC `0x0000`, PCC `0x0000`.
- Signal Context: TSM=1, pktType `0x4`, ICC `0x0000`, PCC `0x0001`, fixed 27 words.
- Version Context: TSM=1, pktType **`0x5`**, ICC `0x0001`, PCC `0x0004`, fixed 11 words.
- Context fixed-point scaling: frequencies/bandwidth/sample-rate = integer · 2²⁰;
  gains/reference levels = integer · 2⁷.
- I/Q packing: components **I then Q**, each two's-complement signed of the context bit
  depth, written **MSB-first** into big-endian 32-bit words, no zero padding.

## Common header values (all three packets)

| Field | Value |
|-------|-------|
| Stream ID | `0x12345678` |
| Integer-seconds timestamp | `1700000000` |
| Fractional-seconds timestamp | `123456789000` ps |

## `signal_context_16bit.difi.bin` (108 bytes, 27 words)

SHA-256: `3681624de6d8be68a72ac2f47f6abb2252f41835e9aafe0711d449cbd8f423f0`

| Field | Value |
|-------|-------|
| Packet type / class | Context `0x4` / `0x0001` |
| CIF0 | `0xFBB98000` (context-changed) |
| Reference point | `0x64` |
| RF reference frequency | `2.401e9` Hz |
| Bandwidth | `1.0e6` Hz |
| Sample rate | `1.25e6` Hz |
| Stage-1 gain | `-3.5` dB |
| Bit depth (via dataPacketFormat) | `16` |

Frequencies/rate/bandwidth are stored as `round(value · 2²⁰)`; gain as `round(dB · 2⁷)`.
Bit depth is carried in `dataPacketFormat.item_packing_field_size` and `data_item_size`
as `(bits − 1) = 15`.

## `version_context.difi.bin` (44 bytes, 11 words)

SHA-256: `36bdf1f512c0042d38875085196c471439ec769a620b14fb1543b92faa076cce`

| Field | Value |
|-------|-------|
| Packet type / class | Version `0x5` / `0x0004` |
| CIF0 | `0x80000002` (context-changed) |
| CIF1 | `0x0000000C` |
| V49 spec version | `0x00000004` |
| Year | `2021` |
| Day | `100` |
| Revision / type / ICD version | `1` / `0` / `0` |

## `signal_data_16bit.difi.bin` (60 bytes)

SHA-256: `a6d30bb5f17a69e9c5bb3f244520d222dd34691a51f97f4ea418d1d1cc5fb871`

| Field | Value |
|-------|-------|
| Packet type / class | Signal Data `0x1` / `0x0000` |
| Sequence number | `0` |
| Bit depth (out-of-band, from context) | `16` |
| Sample count | `8` |

Samples (I, Q): (-4,100) (-3,75) (-2,50) (-1,25) (0,0) (1,-25) (2,-50) (3,-75).

## Regenerating

```sh
# Rust
cd rust && cargo run --bin difi-tool -- gen-golden ../tests/golden

# C++ (regenerates to a temp dir and diffs against these committed bytes)
cd cpp/build && ctest --output-on-failure

# Validate against the DIFI Consortium reference definitions
tests/conformance/setup.sh
/tmp/difi-venv/bin/python3 tests/conformance/certify.py tests/golden
```
