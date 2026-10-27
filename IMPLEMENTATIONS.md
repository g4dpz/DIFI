# Independent DIFI Implementations (1.1 + 1.2 data plane)

Two independent implementations of the DIFI (IEEE-ISTO Std 4900-2021) I/Q codec, built so
they can exchange I/Q over the DIFI wire format and cross-validate each other. Having two
*independent* implementations agree on the byte layout is the core interop story — neither
is "correct by its own definition".

Both implementations cover the **DIFI 1.1 data plane** (Signal Data / Signal Context /
Version Context) and the **DIFI 1.2 data-plane additions** (sample-count classes and
Timing Flow Control) — see "DIFI 1.2 data plane" below. The 1.2 work also builds the
shared Command-Packet core that the forthcoming DIFI 1.3.0 link-establishment control
plane (the version ESTRACK will use) extends.

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

- **DIFI Consortium conformance** (task 10 / Req 3.1, 3.2): both implementations' Signal
  Context (27 words), Version Context (11 words) and Signal Data packets parse and
  validate clean against the Consortium's `construct` definitions, and **dissect cleanly
  in the DIFI Wireshark dissector** (run headless via `tshark`). See
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

Cross-decode against **gr-difi** (both directions) and the **Wireshark dissector check**
(Req 3.1) are both done (see `docs/interop/`). The committed reference signal is a
reproducible LSIS-AFS-*style* waveform; the hosting decision (Req 6.4 / DQ-4) is recorded
in `docs/DATASET-HOSTING-DECISION.md` — external-by-SHA-256 is the ready mechanism for a
real vector. gr-difi end-to-end per-sample loss remains a characterised limitation on
gr-difi's side (`docs/interop/GNURADIO-GRDIFI-EVIDENCE.md`), not our defect.

## DIFI 1.2 data plane (spec `difi-1.2-dataplane`, tasks 1–14)

Both implementations add the DIFI 1.2 data-plane packets alongside the 1.1 ones,
additively — no 1.1 layout, API, or golden packet changed (an explicit backward-compat
regression gate proves it). New in 1.2:

- **Shared Command-Packet core** (`rust/src/command.rs`, `difi::command` in the C++
  header): field-level CAM (Control/Acknowledge Mode), CIF0/CIF1 presence bitmaps,
  Controllee/Controller IDs, and Buffer Size/Status. This is the foundation the DIFI
  1.3.0 link-establishment packets will reuse, not rebuild.
- **Timing Flow Control packets** (`0x0005` Sample Count, `0x0006` Real-Time TSF):
  `TimingFlowControlPacket` — a 21-word Command Packet carrying buffer fill-level
  (OF/NF/NE/UF) and timing. A Sink uses it to pace a Source.
- **Sample-count Data/Context twins** (`0x0002`/`0x0003`): same wire layout as the 1.1
  `0x0000`/`0x0001` packets but sample-count-timestamped (TSF = sample count). Emitted
  via `encode_data_sample_count` / `encode_context_sample_count`.
- **Information-Class recognition** for the 1.2 groupings (`0x0002`/`0x0003`/`0x0004`).

**Running the 1.2 packets:**

```sh
# Both tools generate 7 golden packets (3× 1.1 + 4× 1.2), byte-identical across langs:
rust/target/debug/difi-tool gen-golden <dir>
cpp/build/difi-tool      gen-golden <dir>
# Decode any packet (incl. flow control) to text:
difi-tool dump <packet.bin>
# Live Rust<->C++ flow-controlled exchange (source streams data, sink replies w/ flow control):
difi-tool fc-sink   <data-bind> <fc-dest> <count>
difi-tool fc-source <data-dest> <fc-bind> <count>
```

**Conformance:** all four 1.2 golden packets validate clean against the DIFI Consortium
**v1.2.1** reference definitions (`difi_data/context/command_v1_2_1`) and dissect
correctly in the DIFI Wireshark dissector. Validating against the v1.2.1 reference caught
three real conformance bugs shared by both implementations (CIF0 CIF1-present bit, info/
packet class pairing, sample-count context TSM) — now fixed; see
`tests/conformance/README.md`.

**Interop:** `tests/interop.sh` §1/§2 cover the 1.2 goldens (byte-identity + cross-decode);
§3c runs the live flow-controlled UDP exchange in all four directions; §8 validates all
seven packets against the Consortium reference. Evidence:
`docs/interop/FLOW-CONTROL-EVIDENCE.md`.

**Path to DIFI 1.3.0 / ESTRACK:** 1.1 (shipped) → this 1.2 data plane → 1.3.0 control
plane (link establishment, separate feature) = full DIFI 1.3.0, the version ESTRACK will
use. The 1.2 `command` core is the shared foundation the 1.3.0 packets extend.

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
