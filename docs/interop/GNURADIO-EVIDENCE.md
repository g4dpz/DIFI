# Interop Evidence — GNU Radio (Route 1)

**Requirement:** 7.2 (cross-stack interop, both directions), 7.3 (record evidence).

This exchanges a DIFI stream between our implementations and a **GNU Radio** flowgraph,
over UDP, in both directions. It puts GNU Radio's runtime and its UDP block
(`network.socket_pdu`) genuinely in the path.

## What "GNU Radio" means here (Route 1 vs Route 2)

- **Route 1 (this evidence):** a GNU Radio flowgraph does the networking — `socket_pdu`
  moves one DIFI datagram per PDU — and the DIFI packet logic is the **DIFI Consortium
  reference `construct` definitions** (the same independent code used in
  `tests/conformance/`). GNU Radio's scheduler + UDP block are exercised; the DIFI codec
  on that side is the reference, not GNU Radio's own.
- **Route 2 (not done):** build **gr-difi** — GNU Radio's own DIFI Source/Sink blocks —
  and use those. That is a fully independent DIFI codec. gr-difi is not packaged for
  Homebrew and must be built from source against GNU Radio 3.10 (pybind bindings, API
  changes since its 3.8/3.9-era releases). Deferred; tracked as future work.

## Setup

| Item | Value |
|------|-------|
| GNU Radio | 3.10.12.0 (Homebrew), its bundled venv python |
| Extra dep | `construct` (installed into the GNU Radio venv) |
| Transport | UDP, 127.0.0.1, via GNU Radio `network.socket_pdu` (one PDU per datagram) |
| Signal / mapping | same reference waveform + 16-bit full-scale mapping as the rest of the toolkit |

The GNU Radio side reproduces the reference signal and the float→integer mapping in
Python (`tests/gnuradio/difi_gr_common.py`). Its generated DIFI codes match our tools'
`gen-check` output exactly except **one sample of 4096**, which differs by **1 LSB** —
a float-rounding difference between CPython's `math.cos/sin` and Rust/C++ libm at a
quantisation half-boundary.

## Interop criterion: within 1 LSB (not bit-identical)

Because a peer's float math can differ by a ULP, cross-stack interop is judged by
**recovery within one quantisation step (1 LSB)**, exactly as Req 7.1 intends ("within
the documented quantization loss"), not by bit-identical regeneration. Our `difi-tool`
grew an `import-interop` mode for this (the default `import` stays strict for
same-implementation round trips). The GNU Radio receiver applies the same 1-LSB bound.

## Results — all four directions PASS

| Direction | max_abs error | 1 LSB | Result |
|-----------|---------------|-------|--------|
| rust (export) → GNU Radio (receive) | 1.527e-5 | 3.052e-5 | PASS |
| cpp  (export) → GNU Radio (receive) | 1.527e-5 | 3.052e-5 | PASS |
| GNU Radio (send) → rust (import-interop) | 1.527e-5 | 3.052e-5 | PASS |
| GNU Radio (send) → cpp  (import-interop) | 1.527e-5 | 3.052e-5 | PASS |

All 4096 samples transfer in 16 data packets (+1 Signal Context); recovered error is
~0.5 LSB, well within the 1-LSB interop bound.

## Reproduce

```sh
# one-time: install construct into the GNU Radio venv
<gr-python> -m pip install construct     # gr-python auto-detected by the runner

cd rust && cargo build --bin difi-tool && cd ..
cmake -S cpp -B cpp/build && cmake --build cpp/build
./tests/gnuradio/run_interop.sh
```

The runner skips cleanly if a GNU Radio python with `construct` is not found.

## Caveats

- UDP over **loopback**; not a lossy/reordered link.
- The DIFI codec on the GNU Radio side is the Consortium reference, so this is not yet
  interop with a *fully* independent DIFI implementation. gr-difi (Route 2) or vendor
  hardware (ESTRACK HITL) would be stronger.
- The one-sample 1-LSB difference is inherent to cross-language float math; it is why the
  interop criterion is a quantisation-step tolerance rather than byte equality.
