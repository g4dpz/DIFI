# gr-difi macOS build patches

gr-difi (DIFI-Consortium/gr-difi, GPLv3) does not build cleanly on macOS/ARM against
Homebrew GNU Radio 3.10.12 without two local patches. These are described here (not
applied to any committed copy — gr-difi is fetched into gitignored `work/`). Both are
reasonable upstream fixes.

## 1. Portable endian helpers

gr-difi uses Linux's `htobe64` / `be64toh` (from `<endian.h>`) in
`lib/difi_sink_cpp_impl.h` and `lib/difi_source_cpp_impl.cc`, but never includes the
header and macOS has no `<endian.h>`. Add a portable shim at the top of
`include/difi/difi_common.h` (after the existing includes):

```cpp
#if defined(__APPLE__)
  #include <libkern/OSByteOrder.h>
  #ifndef htobe64
    #define htobe64(x) OSSwapHostToBigInt64(x)
  #endif
  #ifndef be64toh
    #define be64toh(x) OSSwapBigToHostInt64(x)
  #endif
#elif defined(__linux__)
  #include <endian.h>
#else
  #include <sys/endian.h>
#endif
```

## 2. Link spdlog + fmt explicitly

The sources call spdlog directly (via GNU Radio logging). On macOS these symbols are not
pulled in transitively, so `libgnuradio-difi` fails to link
(`spdlog::details::log_msg::log_msg(...)` undefined). In `lib/CMakeLists.txt`, after
`add_library(gnuradio-difi ...)`:

```cmake
find_package(spdlog QUIET)
find_package(fmt QUIET)
if(spdlog_FOUND)
  target_link_libraries(gnuradio-difi spdlog::spdlog)
endif()
if(fmt_FOUND)
  target_link_libraries(gnuradio-difi fmt::fmt)
endif()
```

## 3. (Environment, not a patch) stale fmt headers

If an old Intel-Homebrew `/usr/local` tree is present, its `fmt` v11 headers can shadow
the `/opt/homebrew` v12 headers GNU Radio links against, producing an `fmt::v11` vs
`fmt::v12` symbol mismatch at link time. Force the correct include order with
`-DCMAKE_CXX_FLAGS="-I/opt/homebrew/include"` at configure time.

## 4. (Known limitation) Python bindings don't load

On GR 3.10.12 here, `import difi` fails with `generic_type: type "difi_source_cpp_fc32"
referenced unknown base type "gr::sync_block"` — a pybind type-registry mismatch between
gr-difi's generated bindings and this GNU Radio's pybind layout. We work around it by
using gr-difi's **C++** blocks from a compiled flowgraph (see the .cc files here), which
does not touch the Python bindings.
