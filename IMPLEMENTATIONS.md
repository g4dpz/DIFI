# Independent DIFI 1.1 Implementations

Two independent implementations of the DIFI 1.1 (IEEE-ISTO Std 4900-2021) I/Q codec,
built so they can exchange I/Q over the DIFI wire format and cross-validate each other.
Having two *independent* implementations agree on the byte layout is the core interop
story — neither is "correct by its own definition".

| Dir | Language | Build | Test |
|-----|----------|-------|------|
| `rust/` | Rust (2021) | `cargo build` | `cargo test` |
| `cpp/`  | C++17 (header-only lib) | `cmake -S cpp -B cpp/build && cmake --build cpp/build` | `ctest --test-dir cpp/build` |

Hardening: run the C++ tests (incl. the fuzz harness) under sanitizers with
`cmake -S cpp -B cpp/build-san -DDIFI_SANITIZE=ON && cmake --build cpp/build-san && cpp/build-san/difi_tests`.

Shared, committed in `tests/golden/`:
- Golden DIFI 1.1 packets (Signal Context, Version Context, Signal Data) — the shared
  wire-format oracle. See `tests/golden/MANIFEST.md` for documented field values and
  SHA-256 checksums.

## Scope (spec tasks 1–12)

Each implementation provides a standalone, unit-tested DIFI 1.1 codec, UDP transport,
a float↔integer sample mapper, and a **reproducible signal generator + manifest
writer**, all **validated against the DIFI Consortium's own reference definitions**,
plus an **end-to-end reference-signal export/import** proven across both implementations:

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
  receive so multiple streams can share a port. UDP is DIFI's standard transport. A
  **non-standard TCP dev mode** (Req 5.2) is also provided (`TcpSender`/`TcpReceiver`,
  length-framed, clearly marked non-standard) for lossless ordered exchange. The encoder
  can **segment** a large buffer across multiple Signal Data packets (Req 2.5,
  `encode_data_segmented`).
- **Sample mapper** (task 8 / Req 4): `ScalingConvention`, `float_to_difi`,
  `difi_to_float`, and `measure_implementation_loss`. See the convention below.

- **DIFI Consortium conformance** (task 10 / Req 3.2): both implementations' Signal
  Context (27 words), Version Context (11 words) and Signal Data packets parse and
  validate clean against the Consortium's `construct` definitions. See
  `tests/conformance/`.
- **Cross-decode vs an independent implementation** (task 13 / Req 7.2, 7.3): both
  directions pass against the DIFI Consortium reference code — packets *built* by the
  reference decode in our tools, and our packets parse+validate in the reference.
  Evidence: `docs/interop/CROSS-DECODE-EVIDENCE.md`. (This is the certification
  reference, not gr-difi; see the caveat there.)
- **GNU Radio interop — Route 1** (Req 7.2): a GNU Radio flowgraph exchanges a full DIFI
  stream with both our tools over UDP (`network.socket_pdu`), both directions, recovered
  within 1 LSB. GNU Radio's runtime + UDP block are in the path; the DIFI codec on that
  side is the Consortium reference. Harness: `tests/gnuradio/`; evidence:
  `docs/interop/GNURADIO-EVIDENCE.md`.
- **GNU Radio interop — Route 2, gr-difi** (Req 7.2): interop with gr-difi's OWN DIFI
  blocks (`difi_sink_cpp`/`difi_source_cpp`) — a codec independent of ours and of the
  reference. ours→gr-difi decodes all 4096 samples; gr-difi→ours packets parse in our
  tools and validate in the reference. Verified at the packet/sample-count level (not a
  loss round-trip — gr-difi uses its own scaling); gr-difi's sink is flaky and needed
  macOS build patches. Harness: `tests/gnuradio/route2_grdifi/`; evidence +
  caveats: `docs/interop/GNURADIO-GRDIFI-EVIDENCE.md`.

- **Signal generator + manifest writer** (task 11 / Req 6): a deterministic generator
  reproduces a reference signal from committed parameters; `generate` writes the large
  raw cf32 to a gitignored `work/` dir and a small manifest to the tracked `signals/`
  dir (SHA-256, byte size, sample format, DIFI context params, regen command);
  `verify-manifest` regenerates and confirms the SHA-256 matches. Both implementations
  produce byte-identical raw output. See `signals/README.md`.
- **End-to-end signal export/import** (task 12 / Req 7.1, 7.3): a deterministic
  reference float32 signal is generated from parameters (no raw I/Q committed), mapped
  to DIFI integer, encoded, sent over UDP, decoded, inverse-mapped, and compared against
  the regenerated original. Recovery is within the documented quantization loss
  (end-to-end loss equals mapper-only loss — transport/codec add nothing), in all four
  directions including cross-language. Evidence: `docs/interop/EXPORT-IMPORT-EVIDENCE.md`.

- **Property + fuzz hardening** (task 14 / strengthens Req 1.4, 1.5): random valid
  configs round-trip bit-exact, and the decoder fed tens of thousands of random
  malformed buffers only ever returns a packet or a typed error — never panics or
  mis-parses. Deterministic, seeded, dependency-free (`rust/tests/fuzz.rs`; C++
  `test_fuzz_decoder_never_crashes` / `test_property_iq_round_trip`). The C++ fuzz run
  is verified clean under **AddressSanitizer + UndefinedBehaviorSanitizer**
  (`cmake -DDIFI_SANITIZE=ON`), so the decoder's byte-slice bounds are proven, not just
  reasoned about.

Cross-decode against **gr-difi** or vendor hardware (a stronger form of task 13) and the
Wireshark dissector check (Req 3.1) are the remaining increments. The committed
reference signal is a reproducible LSIS-AFS-*style* waveform; swapping in a real vector's
committed parameters would make the evidence use canonical data without changing the
pipeline (Req 6.4: host the large canonical data externally by SHA-256).

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

`tests/interop.sh` proves (across its numbered sections):
1. Both tools regenerate the golden packets **byte-identically** to the committed oracle.
2. Each tool **decodes the other's** packets and recovers identical field values.
3. **Live UDP exchange in all four directions** (rust→rust, cpp→cpp, cpp→rust,
   rust→cpp): the sender streams one Signal Context + N Signal Data packets over a real
   UDP socket; the receiver filters by Stream ID, decodes, and verifies every sample and
   the mod-16 sequence number. The receiver prints `RECV OK <N>` only on an exact match.
3b. **Non-standard TCP dev-mode exchange, all four directions** (Req 5.2): the same
   verification over the length-framed TCP transport, including cross-language
   (rust↔cpp), proving the framing is byte-compatible between implementations.
4. **Sample-mapper agreement**: both tools map the same fixed float vector to integer
   codes at bit depths 4/8/12/16 and must produce byte-identical code lists and loss
   figures.
5. **Generated-signal agreement**: both tools' generated + mapped DIFI codes are
   bit-identical.
6. **End-to-end export/import**: a reference signal survives generate → map → encode →
   UDP → decode → inverse-map within the documented quantization loss, all four
   directions.
7. **DIFI Consortium conformance**: both tools' packets pass the Consortium reference
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
