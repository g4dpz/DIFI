# Quick Start

From clone to exchanging a DIFI packet in a few minutes. For the full feature list see
[`IMPLEMENTATIONS.md`](IMPLEMENTATIONS.md); for interop evidence see
[`docs/interop/`](docs/interop/); new to DIFI itself? read
[`docs/reference/DIFI-VITA49-PRIMER.md`](docs/reference/DIFI-VITA49-PRIMER.md).

## Prerequisites

| For | Need |
|-----|------|
| Rust implementation | Rust toolchain (`cargo`), 1.75+ (tested on 1.88) |
| C++ implementation | CMake 3.16+ and a C++17 compiler (tested: clang 21) |
| External conformance (optional) | Python 3 + `tshark`/Wireshark (for Req 3.1) |

You only need one of Rust / C++ to get started; the cross-language interop checks use both.

## 1. Clone and build

```sh
git clone https://github.com/g4dpz/DIFI.git
cd DIFI

# Rust
cd rust && cargo build && cd ..

# C++
cmake -S cpp -B cpp/build && cmake --build cpp/build
```

Each build produces a `difi-tool` CLI:
- Rust: `rust/target/debug/difi-tool`
- C++:  `cpp/build/difi-tool`

(Below, `difi-tool` means either one — they are interchangeable.)

## 2. Run the tests

```sh
cd rust && cargo test && cd ..                 # Rust unit/integration tests
ctest --test-dir cpp/build                     # C++ tests
./tests/interop.sh                             # cross-implementation interop suite
```

The interop suite's later sections (DIFI Consortium conformance, Wireshark dissector,
GNU Radio) skip cleanly if their optional tooling is absent — run
`tests/conformance/setup.sh` first to enable them.

## 3. Make and inspect a DIFI packet

```sh
# Write the three golden packets (Signal Context, Version Context, Signal Data) to a dir
difi-tool gen-golden /tmp/difi

# Decode one and print its fields
difi-tool dump /tmp/difi/signal_context_16bit.difi.bin
```

Expected:

```
SignalContext stream=0x12345678 class=0x0001
  rf=2401000000 Hz bw=1000000 Hz fs=1250000 Hz gain=-3.5 dB bit_depth=16
```

## 4. Exchange I/Q over UDP (two terminals)

Start the receiver first, then the sender. `send`/`recv` take `<host:port> <packet-count>`.

```sh
# terminal 1 — receive and verify 8 data packets
difi-tool recv 127.0.0.1:4991 8

# terminal 2 — send 1 Signal Context + 8 Signal Data packets
difi-tool send 127.0.0.1:4991 8
```

Receiver prints `RECV OK 8` on an exact match. This works **cross-language** — e.g. the
Rust `difi-tool` sending to the C++ `difi-tool` receiving (that is section 3 of
`tests/interop.sh`).

A non-standard, length-framed **TCP** dev transport is also available (`tcp-send` /
`tcp-recv`, same arguments) for lossless, ordered exchange during development. TCP is not
DIFI-standard transport and will not interoperate with a standard UDP endpoint.

## 5. End-to-end: export a reference signal and recover it

```sh
# terminal 1 — receive, decode, inverse-map, and report quantization loss
difi-tool import 127.0.0.1:4992

# terminal 2 — generate a reference signal, map to DIFI integer, stream it
difi-tool export 127.0.0.1:4992
```

The receiver reports the measured recovered-vs-original loss and `IMPORT OK` when within
the documented quantization bound. Use `import-interop` instead of `import` to judge
within one quantization step (1 LSB) — appropriate when the peer is a different
implementation whose float maths may differ by a ULP.

## 6. Reproducible signals + manifest (no large raw I/Q in Git)

```sh
difi-tool generate work                                   # raw cf32 -> work/ (gitignored)
                                                          # manifest -> signals/ (committed)
difi-tool verify-manifest signals/afs-style-tone-4096.manifest.json
```

`verify-manifest` regenerates the signal from the committed parameters and checks the
SHA-256 matches — the artifact-policy guarantee that the signal is reproducible without
storing it.

## 7. External conformance (optional)

```sh
tests/conformance/setup.sh                                # fetch DIFI reference defs + dissector, build venv

# validate our packets against the DIFI Consortium reference definitions
/tmp/difi-venv/bin/python3 tests/conformance/certify.py tests/golden

# validate our packets in the DIFI Wireshark dissector (needs tshark)
python3 tests/conformance/wireshark_check.py tests/golden
```

GNU Radio interop (both `socket_pdu` and gr-difi's own blocks) lives under
`tests/gnuradio/` — see its README.

## `difi-tool` subcommands

```
gen-golden <dir>            write the three golden packets to <dir>
dump <file>                decode a raw packet file and print its fields
send <host:port> <count>   stream context + <count> data packets over UDP
recv <bind> <count>        receive and verify <count> data packets over UDP
tcp-send / tcp-recv        same, over the non-standard TCP dev transport
export <host:port>         stream a reference signal (generate -> map -> encode -> UDP)
import <bind>              receive a signal, recover it, report quantization loss
import-interop <bind>      as import, but pass within 1 LSB (cross-implementation)
map-check <depth>          print float->integer mapped codes at a bit depth
gen-check                  print the generated reference signal's DIFI codes
generate <work-dir>        write raw cf32 (gitignored) + manifest (committed)
verify-manifest <file>     regenerate and confirm the manifest SHA-256
```

## Troubleshooting

- **`recv`/`import` hangs** — start the receiver *before* the sender; it waits for the
  expected number of packets (UDP) or a connection (TCP).
- **Interop sections "skip"** — that is expected without the optional tooling; run
  `tests/conformance/setup.sh`, and install Wireshark for the dissector check.
- **`tshark` not found on macOS** — it ships inside the app bundle at
  `/Applications/Wireshark.app/Contents/MacOS/`; the harness auto-detects it there, or
  set `DIFI_TSHARK`.
