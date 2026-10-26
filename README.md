# DIFI I/Q Interoperability Toolkit

A reference **DIFI (Digital IF Interoperability)** I/Q exchange layer for CCSDS
community prototyping — so independent reference implementations (and, later, ESTRACK
hardware) can exchange digitized I/Q over a single standard interface.

DIFI is a tightly constrained profile of **VITA 49.2 (VRT)** over UDP/IP
(IEEE-ISTO Std 4900-2021). Adopting it as the common I/Q interchange format removes
proprietary interfaces between digitizers, modems and signal-processing software, and
opens a clean path to **hardware-in-the-loop** testing with ESA's all-digital ground
station (ESTRACK).

## Context

- Part of the CCSDS community prototyping effort coordinated at
  **[esa/CCSDS-Community-Prototyping](https://github.com/esa/CCSDS-Community-Prototyping)**
  — a hub of *independent* implementations that test together (not a single monorepo).
- Proposed direction: use DIFI as **the** interoperability exchange format across
  implementations, with ESTRACK hardware-in-the-loop as the second step.

## What this project provides

**Two independent implementations** of DIFI — one in **Rust** (`rust/`), one in **C++17**
(`cpp/`) — that encode/decode the DIFI packet types over UDP. Built as two separate
implementations deliberately, so interoperability is *demonstrated*, not asserted.

They cover the **DIFI 1.1 data plane** (Signal Data, Signal Context, Version Context) and
the **DIFI 1.2 data-plane additions** — sample-count classes (`0x0002`/`0x0003`) and
**Timing Flow Control** packets (`0x0005`/`0x0006`), built on a shared Command-Packet core.
The 1.2 work is the foundation for the forthcoming **DIFI 1.3.0** link-establishment
control plane — the version ESA's ESTRACK will use (build order: 1.1 → 1.2 → 1.3.0).

Validated — all automated and reproducible:

- **DIFI Consortium reference code** — packets validate against the Consortium's own
  `construct` packet definitions; bidirectional cross-decode.
- **DIFI Wireshark dissector** — packets dissect correctly in `difi-dissector.lua` (run
  headless via `tshark`), with decoded field values matching what was encoded.
- **gr-difi** — interop with GNU Radio's own DIFI Source/Sink blocks, both directions.
- **End-to-end** — a reference signal generated → mapped to DIFI integer → sent over UDP
  → received → recovered within the documented quantization loss, including cross-language.
- **Robustness** — fail-closed decoder, fuzz-tested; C++ clean under ASan/UBSan.

Also included: a float↔integer **sample mapper** with measured loss, a non-standard **TCP
dev transport**, **reproducible signal generators + manifests** (no large raw I/Q in Git),
and a small set of committed **golden packets** as a shared interop checkpoint.

See **[`IMPLEMENTATIONS.md`](IMPLEMENTATIONS.md)** for the full feature list and
**[`docs/interop/`](docs/interop/)** for the interop evidence.

## Structure

| Path | Purpose |
|------|---------|
| `rust/` | Rust implementation (library + `difi-tool` CLI) |
| `cpp/` | C++17 implementation (header-only library + `difi-tool` CLI) |
| `tests/golden/` | Committed golden DIFI packets + manifest (the shared interop oracle) |
| `tests/interop.sh` | Cross-implementation interop suite (generation, cross-decode, UDP/TCP, export/import, conformance, Wireshark) |
| `tests/conformance/` | Validation against the DIFI Consortium reference + Wireshark dissector |
| `tests/gnuradio/` | GNU Radio interop (socket_pdu, and gr-difi's own blocks) |
| `docs/interop/` | Recorded interop evidence |
| `docs/reference/DIFI-VITA49-PRIMER.md` | Condensed DIFI + VITA 49.2 primer and source list |
| `.kiro/specs/difi-iq-interop/` | The spec: `requirements.md`, `design.md`, `tasks.md` |
| `.kiro/steering/artifact-policy.md` | Mandatory repo-size / artifact rules |

## Quick start

```sh
# Rust
cd rust && cargo test && cd ..

# C++
cmake -S cpp -B cpp/build && cmake --build cpp/build && ctest --test-dir cpp/build

# Cross-implementation interop (builds both tools; some sections need optional tooling)
# Covers 1.1 + 1.2: golden byte-identity, cross-decode, live UDP, flow control (§3c),
# and conformance vs the DIFI v1.2.1 reference + Wireshark dissector.
./tests/interop.sh

# DIFI 1.2 flow control, live Rust<->C++ (source streams data, sink replies w/ flow control):
#   difi-tool fc-sink <data-bind> <fc-dest> <count>   # one end
#   difi-tool fc-source <data-dest> <fc-bind> <count> # the other

# Optional external checks (fetch the DIFI reference defs + dissector, need Python/tshark)
tests/conformance/setup.sh
```

See **[`QUICKSTART.md`](QUICKSTART.md)** for a step-by-step walkthrough (clone → build →
exchange a DIFI packet → end-to-end signal → conformance).

New to DIFI/VITA 49.2? Read **[`docs/reference/DIFI-VITA49-PRIMER.md`](docs/reference/DIFI-VITA49-PRIMER.md)** first.

## Open decisions (for the community)

These affect interoperability and are intentionally left for the group to agree:

- **DIFI version** — 1.1 is implemented; 1.2.1 (flow control, command packets) is a
  future increment (what will ESTRACK expect?).
- **Sample format** — bit depth + scaling/reference-level convention (16-bit full-scale
  today; to be agreed against the DIFI reference-gain guidance).
- **Canonical datasets** — host a few agreed reference I/Q vectors externally by checksum.

## Repository-size discipline

Large raw I/Q is **never** committed to Git. We commit generators + manifests + small
golden packets, and host large canonical datasets externally by checksum. See
`.kiro/steering/artifact-policy.md`.

## Licence

Licensed under the **Apache License, Version 2.0** — see [`LICENSE`](LICENSE) and
[`NOTICE`](NOTICE). Apache-2.0 is permissive and compatible with contribution to the ESA
community hub.

Note: `tests/conformance/` and `tests/gnuradio/` *fetch* third-party DIFI Consortium /
gr-difi code on demand into a gitignored dir; that code is **not** redistributed here and
remains under its own licence (see `NOTICE`).
