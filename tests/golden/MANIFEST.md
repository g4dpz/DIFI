# Golden DIFI Packets — Shared Conformance Oracle

These packets are the **shared wire-format oracle** for every independent implementation
in this toolkit (Rust, C++). They are small (≤108 bytes each) and are committed
deliberately (the artifact policy allowlists `tests/golden/`). The first three are the
DIFI **1.1** data-plane packets; the last four are the DIFI **1.2** additions
(sample-count twins and Timing Flow Control). Any implementation claiming conformance MUST:

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

## DIFI 1.2 packets

The four packets below are the DIFI 1.2 additions. They share the common header values
above (Stream ID `0x12345678`, integer-seconds `1700000000`) and the context fixed-point
scaling. The sample-count packets carry a **sample count** in the fractional-timestamp
field (TSF = sample count, `0b01`), not picoseconds.

### `sample_count_context_16bit.difi.bin` (108 bytes, 27 words)

SHA-256: `91e6d1fdc03a4024dac5b9fedc6372564ce6e5759948bfc2281b6687e7c65d12`

Byte-identical to `signal_context_16bit.difi.bin` **except** word 0 (TSF nibble = sample
count, **TSM = 0** per the v1.2.1 reference for class `0x0003`) and word 3 (Information
Class `0x0002`, Packet Class `0x0003`). Same RF/bandwidth/sample-rate/gain/bit-depth
values. Fractional field = `0`. Validates clean against the DIFI Consortium v1.2.1
`difi_context_v1_2_1` reference.

### `sample_count_data_16bit.difi.bin` (60 bytes)

SHA-256: `7cef986b9b198480c36532d35e17713a1c760e053a7ea55ac1d8eeacbb1a5d32`

Same eight samples as `signal_data_16bit.difi.bin`; Information Class `0x0002`, Packet
Class `0x0002`, TSF = sample count, fractional field = sample count `4096`.

### `timing_flow_control_sc.difi.bin` (84 bytes, 21 words) — Packet Class `0x0005`
### `timing_flow_control_rt.difi.bin` (84 bytes, 21 words) — Packet Class `0x0006`

SHA-256 (sc): `25cdc3322933a28d88585d0a5cf62c7ba0f467d8961f1cd0d64fea8d010c3e99`
SHA-256 (rt): `da17d53d19e68050dd579bb78219567336fdba395c294ff8770611a6147aee9e`

VITA 49.2 Command Packets (VRT type `0x6`). Per the v1.2.1 reference, the Information
Class pairs with the Packet Class: **`0x0005` uses Information Class `0x0002`** (and
TSF = sample count), **`0x0006` uses Information Class `0x0003`** (and TSF = real-time ps).
Both validate clean against `difi_command_v1_2_1`. Field values:

| Field | Value |
|-------|-------|
| CAM | Controllee + Controller ID present, Action Mode = execute (`0b10`) |
| Message ID | `1` |
| Controllee ID / Controller ID | `0x000000A1` / `0x000000B2` |
| Reference Point | `0x64` |
| Sample Rate | `1.25e6` Hz (stored `round(value · 2²⁰)`) |
| Timestamp Adjustment | `2500` fs |
| Buffer Size | `1048576` bytes (`1 << 20`) |
| Buffer Status | Near-Full set; buffer level `0x00300000` |

CIF0 declares Reference Point (bit 30) + Sample Rate (bit 21) + Timestamp Adjustment
(bit 20) + CIF1 present (**bit 1**, per the v1.2.1 reference) → CIF0 = `0x40300002`; CIF1
declares Buffer Size present (bit 1). Word order: CAM(8), Message/Controllee/Controller
IDs (9-11), CIF0(12), CIF1(13), Reference Point(14), Sample Rate(15-16), Timestamp
Adjustment(17-18), Buffer Size(19-20), Buffer Status(21).

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
