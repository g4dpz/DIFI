# Interop Evidence — Reference Signal Export/Import over DIFI

**Requirements:** 7.1 (export → re-import matches within documented quantization loss),
7.3 (record interop results as commands, fields, pass/fail for the community hub).

This records a reference float32 I/Q signal being exported to DIFI and re-imported,
across two independent implementations exchanging packets over UDP.

## Setup

| Item | Value |
|------|-------|
| Rust | rustc 1.88.0 |
| C++ | Apple clang 21 (C++17) |
| Signal | deterministic generator, 4096 complex float32 samples, peak 0.9, fs 1.25 MHz, tones +50 kHz / −120 kHz |
| Mapping | 16-bit signed, full_scale = 1.0 (1 LSB = 3.052e-5) |
| Transport | UDP, 127.0.0.1, 256 samples/packet → 16 data packets + 1 Signal Context |

The signal is **regenerated from parameters** on both ends (artifact policy: no raw I/Q
committed). Section 5 of `tests/interop.sh` confirms the generated+mapped DIFI integer
codes are **bit-identical** between the two implementations, so the receiver's comparison
against its own regenerated original is valid cross-language.

## Pipeline

```
generate float32  →  map to 16-bit signed (ScalingConvention)  →  encode Signal Context
+ chunked Signal Data  →  UDP  →  decode  →  inverse-map to float32  →  compare vs original
```

## Commands

```sh
# receiver (import) first, then sender (export); 127.0.0.1:<port>
difi-tool import 127.0.0.1:55401     # Rust or C++
difi-tool export 127.0.0.1:55401     # the other implementation
```

## Results — all four directions PASS

| Direction (export → import) | Samples | Packets | End-to-end max_abs | End-to-end rms | Result |
|------------------------------|---------|---------|--------------------|----------------|--------|
| rust → rust | 4096 | 16 + ctx | 1.526e-5 | 8.783e-6 | PASS |
| cpp  → cpp  | 4096 | 16 + ctx | 1.526e-5 | 8.783e-6 | PASS |
| **cpp  → rust** | 4096 | 16 + ctx | 1.526e-5 | 8.783e-6 | PASS |
| **rust → cpp**  | 4096 | 16 + ctx | 1.526e-5 | 8.783e-6 | PASS |

Representative receiver report (cpp → rust):

```
IMPORT samples=4096 packets_ok
  documented mapping: 16-bit signed, full_scale=1, 1 LSB=3.052e-5
  mapper-only loss:  max_abs=1.526e-5 rms=8.783e-6
  end-to-end loss:   max_abs=1.526e-5 rms=8.783e-6
IMPORT OK — recovered within documented quantization loss
```

## Interpretation

- **End-to-end loss == mapper-only loss** in every direction. The encode → UDP → decode
  path added **zero** error; the only loss is the documented float→16-bit quantization.
- Max per-component error is **0.5 LSB** (1.526e-5 = half of 3.052e-5), the expected
  worst case for round-half-away-from-zero quantization at 16-bit full-scale.
- The cross-language directions (cpp↔rust) prove the exchange works between *independent*
  implementations, which is the point of DIFI.

## How to reproduce

```sh
cd rust && cargo build --bin difi-tool && cd ..
cmake -S cpp -B cpp/build && cmake --build cpp/build
./tests/interop.sh        # sections 5 and 6 cover this evidence
```

Pinned to repo commit `9d90e3b` (implementations) + this change.

## Caveats

- UDP over **loopback** with a 2 ms inter-packet gap; not a lossy/reordered link. The
  sequence-number check is present but packet loss is not injected in this test.
- The signal is a reproducible LSIS-AFS-*style* float32 waveform (tone sum + envelope),
  not a real LSIS-AFS vector. Swapping in a real vector's committed generator parameters
  (task 11) would make this evidence use canonical data; the pipeline is unchanged.
