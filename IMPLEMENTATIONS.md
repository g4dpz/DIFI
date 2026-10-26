# Independent DIFI 1.1 Implementations

Two independent implementations of the DIFI 1.1 (IEEE-ISTO Std 4900-2021) I/Q codec,
built so they can exchange I/Q over the DIFI wire format and cross-validate each other.
Having two *independent* implementations agree on the byte layout is the core interop
story — neither is "correct by its own definition".

| Dir | Language | Build | Test |
|-----|----------|-------|------|
| `rust/` | Rust (2021) | `cargo build` | `cargo test` |
| `cpp/`  | C++17 (header-only lib) | `cmake -S cpp -B cpp/build && cmake --build cpp/build` | `ctest --test-dir cpp/build` |

Shared, committed in `tests/golden/`:
- Golden DIFI 1.1 packets (Signal Context, Version Context, Signal Data) — the shared
  wire-format oracle. See `tests/golden/MANIFEST.md` for documented field values and
  SHA-256 checksums.

## Scope (spec tasks 1–10)

Each implementation provides a standalone, unit-tested DIFI 1.1 codec, UDP transport,
and a float↔integer sample mapper, all **validated against the DIFI Consortium's own
reference definitions**:

- Constants for the DIFI 1.1 profile (VITA OUI `0x6A621E`, 7-word header, class codes).
- Packet model: `SignalDataPacket`, `SignalContextPacket`, `VersionContextPacket` with
  `pack`/`unpack`, big-endian, 32-bit-word aligned.
- I/Q sample packing for signed-integer bit depths **4–16**, including sub-byte packing
  with no zero padding (MSB-first, I then Q).
- Fail-closed decoder: rejects non-VITA OUI, packet-size/length mismatch, bad class,
  bad bit depth — with typed, field-level errors; never mis-parses.
- Encoder with mod-16 stream sequencing and config validation.
- Golden packets as regression fixtures.
- **UDP transport** (task 9 / Req 5): `UdpSender` / `UdpReceiver`, decoupled from the
  packet model (one DIFI packet per UDP datagram), with **Stream-ID filtering** on
  receive so multiple streams can share a port. UDP is DIFI's standard transport; a
  non-standard TCP dev mode (Req 5.2) is not implemented yet.
- **Sample mapper** (task 8 / Req 4): `ScalingConvention`, `float_to_difi`,
  `difi_to_float`, and `measure_implementation_loss`. See the convention below.

- **DIFI Consortium conformance** (task 10 / Req 3.2): both implementations' Signal
  Context (27 words), Version Context (11 words) and Signal Data packets parse and
  validate clean against the Consortium's vendored `construct` definitions. See
  `tests/conformance/`.

Exporting a real reference signal end-to-end (task 12) and the Wireshark dissector check
(Req 3.1) are the next increments and are not yet built.

## Conformance finding (why task 10 mattered)

The first versions of these implementations passed all their own tests and agreed
byte-for-byte with each other, yet their Signal Context and Version Context packets were
**not DIFI-conformant** — the layouts were a reasonable guess, not the standard's. The
external oracle caught it. Both were rewritten to the authoritative 27-word context and
11-word version layouts (Version Context uses VRT packet type `0x5`, context fields use
fixed-point 2²⁰/2⁷ scaling, bit depth lives in `dataPacketFormat`). Details in
`tests/conformance/README.md`. Two implementations agreeing proves interop; only an
external reference proves standards conformance.

## Sample-mapping convention (DQ-2 / Req 4.3)

A float sample is a normalised amplitude where **`full_scale` maps to the positive
full-scale integer code** `FS = 2^(n-1) - 1` for bit depth `n`, applied symmetrically
to I and Q:

- **float → int:** `code = round(clamp(x / full_scale, -1, 1) * FS)`, round-half-away-
  from-zero. Over-range input **saturates** to ±FS; it never wraps.
- **int → float:** `x = code / FS * full_scale`.

`full_scale` is the real-world amplitude at 0 dBFS, recorded in the signal manifest.
`full_scale = 1.0` is the normalised ±1.0 convention and the default.

The round trip is **not lossless** (quantisation). `measure_implementation_loss`
reports max-abs error, RMS error, and SNR in dB — loss is always measured, never
assumed zero. Both implementations produce **bit-identical integer codes** for the same
float input at every bit depth (verified by `tests/interop.sh` section 4).

## Shared wire-format decisions

- DIFI **1.1**; **16-bit signed, full-scale** default sample format.
- Timestamps: TSI = UTC, TSF = real-time picoseconds.
- Information Class `0x0000`; Packet Classes: data `0x0000`, signal context `0x0001`,
  version context `0x0004`.

Any change to the wire format must be made in **both** implementations and must keep the
golden packets byte-identical, or the committed golden fixtures must be regenerated and
the manifest updated deliberately.

## Verifying interop

With both tools built:

```sh
cd rust && cargo build --bin difi-tool && cd ..
cmake -S cpp -B cpp/build && cmake --build cpp/build
./tests/interop.sh
```

`tests/interop.sh` proves three things:
1. Both tools regenerate the golden packets **byte-identically** to the committed oracle.
2. Each tool **decodes the other's** packets and recovers identical field values.
3. **Live UDP exchange in all four directions** (rust→rust, cpp→cpp, cpp→rust,
   rust→cpp): the sender streams one Signal Context + N Signal Data packets over a real
   UDP socket; the receiver filters by Stream ID, decodes, and verifies every sample and
   the mod-16 sequence number. The receiver prints `RECV OK <N>` only on an exact match.
4. **Sample-mapper agreement**: both tools map the same fixed float vector to integer
   codes at bit depths 4/8/12/16 and must produce byte-identical code lists and loss
   figures.
5. **DIFI Consortium conformance**: both tools' packets pass the Consortium reference
   validation (skipped automatically if the conformance venv is absent — run
   `tests/conformance/setup.sh`).

Expected output:

```
== 3. live UDP interop (all four directions) ==
  ok  rust -> rust  (RECV OK 8)
  ok  cpp  -> cpp  (RECV OK 8)
  ok  cpp  -> rust  (RECV OK 8)
  ok  rust -> cpp  (RECV OK 8)
== 4. sample-mapper agreement (float -> DIFI integer) ==
  ok  bit_depth=4  (rust codes == cpp codes, identical loss)
  ...
ALL INTEROP CHECKS PASSED
```
