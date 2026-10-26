# GNU Radio Interop — Route 2 (gr-difi's own DIFI blocks)

Route 2 interoperates with **gr-difi** — GNU Radio's own DIFI Source/Sink blocks, a
DIFI codec independent of both our implementations *and* the DIFI Consortium reference.
This is the strongest interop evidence short of vendor hardware.

Unlike Route 1 (which used GNU Radio's UDP block + the Consortium reference definitions
for the DIFI logic), here the DIFI encode/decode is done by gr-difi's compiled C++ blocks
`gr::difi::difi_sink_cpp` / `difi_source_cpp`, which also do their own UDP.

## Status: works at the packet/stream level, with caveats

- **ours → gr-difi:** our `difi-tool export` streams a DIFI signal; gr-difi's
  `difi_source_cpp` decodes all 4096 samples. ✅ Reproducible.
- **gr-difi → ours:** gr-difi's `difi_sink_cpp` emits DIFI packets that our Rust and C++
  decoders parse correctly (stream 0x12345678, 16-bit, 256 samples/packet) and that the
  DIFI Consortium reference also validates clean. ✅ when the sink runs.

### Caveats (important)

1. **The gr-difi sink is flaky on this setup.** `difi_sink_cpp` intermittently aborts
   with a `trace trap` on teardown; some runs send the full stream, some crash early.
   This is a gr-difi stability issue, not ours. The ours→gr-difi direction (gr-difi as
   *source*) is stable.
2. **Sample values are not loss-compared across gr-difi.** gr-difi applies its own
   float↔integer scaling (`scaling`/`max_iq`/`min_iq` args), which differs from our
   documented full-scale convention. So gr-difi↔ours is verified at the **packet and
   sample-count level**, not by end-to-end quantization loss. Matching the scaling
   conventions to do a true loss comparison is follow-up work.
3. **gr-difi needed macOS patches to build** (see below); it is not a clean upstream
   build on this platform.

## Building gr-difi (not committed here)

gr-difi is GPLv3 third-party code; we do not vendor it. Build it into the gitignored
`work/` area:

```sh
GRPY=$(ls /opt/homebrew/Cellar/gnuradio/*/libexec/venv/bin/python | head -1)
"$GRPY" -m pip install pybind11 numpy
git clone https://github.com/DIFI-Consortium/gr-difi work/gr-difi
# Two local patches are required on macOS (see PATCHES.md):
#   1. endian shim (htobe64/be64toh) in include/difi/difi_common.h
#   2. link spdlog+fmt in lib/CMakeLists.txt
cd work/gr-difi && mkdir build
cmake -S . -B build \
  -DCMAKE_PREFIX_PATH="/opt/homebrew;/opt/homebrew/opt/gnuradio;$("$GRPY" -c 'import pybind11;print(pybind11.get_cmake_dir())')" \
  -Dfmt_DIR=/opt/homebrew/lib/cmake/fmt -Dspdlog_DIR=/opt/homebrew/lib/cmake/spdlog \
  -DPYTHON_EXECUTABLE="$GRPY" -DCMAKE_INSTALL_PREFIX="$PWD/install" \
  -DCMAKE_CXX_FLAGS="-I/opt/homebrew/include"   # force fmt v12 over stale /usr/local fmt v11
cmake --build build -j4 && cmake --install build
```

## Building and running the flowgraphs

The C++ flowgraph sources are committed in this directory (`gr_difi_tx.cc`,
`gr_difi_rx.cc`). `build_and_run.sh` compiles them against the gr-difi install in
`work/` and runs both directions against our `difi-tool`.

```sh
./tests/gnuradio/route2_grdifi/build_and_run.sh
```

The Python bindings (`import difi`) do **not** load on this GR 3.10.12 (a pybind
"unknown base type gr::sync_block" registry mismatch), so we use gr-difi's **C++** blocks
directly from a compiled flowgraph, which sidesteps the Python-binding problem entirely.
