# Interop Evidence — gr-difi (GNU Radio Route 2)

**Requirement:** 7.2 (cross-stack interop, both directions, with a second *independent*
DIFI implementation), 7.3 (record evidence).

This interoperates with **gr-difi** — GNU Radio's own DIFI Source/Sink blocks. gr-difi is
a DIFI codec independent of *both* our implementations and the DIFI Consortium reference
`construct` definitions. This is the strongest interop evidence in the repo short of
vendor hardware.

## Route 1 vs Route 2

- **Route 1** (`GNURADIO-EVIDENCE.md`): GNU Radio's runtime + UDP block, but the DIFI
  packet logic was the Consortium reference.
- **Route 2 (this):** the DIFI encode/decode is done by gr-difi's compiled C++ blocks
  `gr::difi::difi_sink_cpp` / `difi_source_cpp` — its own, independent codec.

## Results

| Direction | Result |
|-----------|--------|
| ours (`difi-tool export`) → gr-difi (`difi_source_cpp`) | ✅ gr-difi decoded all 4096 samples |
| gr-difi (`difi_sink_cpp`) → ours (Rust + C++ decoders) | ✅ packets parse correctly; DIFI reference also validates them |

A gr-difi-emitted Signal Data packet decoded by our Rust tool:

```
SignalData stream=0x12345678 seq=0 bit_depth=16 samples=256
```

The same packet validates clean under the DIFI Consortium reference — so three
independent implementations (gr-difi, ours, the certification reference) agree on
gr-difi's bytes.

## How it was built (summary)

gr-difi does not build cleanly on macOS/ARM against Homebrew GNU Radio 3.10.12. Getting
it to build required (details in `tests/gnuradio/route2_grdifi/PATCHES.md`):

1. A portable `htobe64`/`be64toh` shim (macOS lacks `<endian.h>`).
2. Linking spdlog + fmt explicitly (`libgnuradio-difi` otherwise fails to link).
3. Forcing `/opt/homebrew/include` ahead of a stale Intel-Homebrew `/usr/local` fmt v11
   that otherwise causes an `fmt::v11` vs `fmt::v12` ABI mismatch.

gr-difi's **Python bindings do not load** on this GR 3.10.12 (`import difi` →
`unknown base type "gr::sync_block"`, a pybind registry mismatch). We therefore use
gr-difi's **C++ blocks directly from a compiled flowgraph**
(`tests/gnuradio/route2_grdifi/gr_difi_{tx,rx}.cc`), which sidesteps the Python problem.

## Caveats (important — this is partial, not clean, interop)

1. **The gr-difi sink is flaky.** `difi_sink_cpp` intermittently aborts with a
   `trace trap` on teardown; some runs crash before sending. The harness retries and
   judges the gr-difi→ours direction by packet decode. The ours→gr-difi direction
   (gr-difi as source) is stable.
2. **No end-to-end loss comparison across gr-difi.** gr-difi applies its own float↔int
   scaling, different from our documented full-scale convention, so gr-difi↔ours is
   verified at the **packet and sample-count level**, not by quantization loss. Aligning
   the scaling conventions for a true loss round-trip is follow-up work.
3. gr-difi required local build patches; this is not a clean upstream build on macOS.

## Reproduce

Build gr-difi into gitignored `work/` (see `tests/gnuradio/route2_grdifi/README.md` +
`PATCHES.md`), then:

```sh
./tests/gnuradio/route2_grdifi/build_and_run.sh
```

It skips cleanly if GNU Radio / the gr-difi install is absent.

## Bottom line

Route 2 demonstrates real interop with an independent third-party DIFI codec (gr-difi):
our encoder's stream is fully decoded by gr-difi, and gr-difi's packets are decoded by
our tools and validated by the reference. The remaining gaps — the flaky gr-difi sink,
and matching scaling conventions for a loss comparison — are gr-difi/tuning issues, not
defects in our implementations.
