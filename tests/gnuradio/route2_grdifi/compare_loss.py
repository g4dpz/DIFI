#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Measure end-to-end loss of our-export -> gr-difi-decode (Route 2, task 11).

gr-difi's difi_source_cpp decodes DIFI data packets to gr_complex WITHOUT scaling: each
int16 I/Q code is returned as a float directly. So for a 16-bit export using this
toolkit's convention (full-scale denominator FS = 2^(n-1)-1 = 32767), gr-difi emits
floats in +/-32767. This script renormalises gr-difi's recovered samples by FS to get
back +/-1.0, then compares against the original reference signal and reports the loss.

Usage: compare_loss.py <original.cf32> <gr_difi_recovered.cf32> [bit_depth]
"""
import struct
import sys
import math


def read_cf32(path):
    with open(path, "rb") as f:
        raw = f.read()
    n = len(raw) // 8
    vals = struct.unpack("<" + "f" * (n * 2), raw[: n * 8])
    return [(vals[2 * k], vals[2 * k + 1]) for k in range(n)]


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    original = read_cf32(sys.argv[1])
    recovered_raw = read_cf32(sys.argv[2])
    bit_depth = int(sys.argv[3]) if len(sys.argv) > 3 else 16
    fs = (1 << (bit_depth - 1)) - 1  # 32767 for 16-bit — this toolkit's FS convention

    if len(original) != len(recovered_raw):
        print(f"FAIL: sample-count mismatch — original {len(original)}, "
              f"recovered {len(recovered_raw)} (a lossless decode must recover every "
              f"sample, not a prefix)")
        return 1
    n = len(original)
    if n == 0:
        print("FAIL: no samples to compare")
        return 1

    sum_sq_err = 0.0
    sum_sq_sig = 0.0
    max_abs = 0.0
    for k in range(n):
        oi, oq = original[k]
        # Renormalise gr-difi's raw int16-as-float codes back to +/-1.0.
        ri, rq = recovered_raw[k][0] / fs, recovered_raw[k][1] / fs
        for o, r in ((oi, ri), (oq, rq)):
            e = r - o
            sum_sq_err += e * e
            sum_sq_sig += o * o
            max_abs = max(max_abs, abs(e))

    rms = math.sqrt(sum_sq_err / (n * 2))
    snr_db = float("inf") if sum_sq_err == 0 else 10.0 * math.log10(sum_sq_sig / sum_sq_err)
    one_lsb = 1.0 / fs
    # Expected worst case: one-code quantization difference. gr-difi's sink truncates
    # toward zero where our export rounds half away from zero, so beyond our own ~0.5-LSB
    # quantization there can be up to ~1 extra LSB of disagreement at some samples — but
    # here gr-difi is only DECODING our bytes (lossless int16->float), so the only error
    # is our float->int16 quantization on export: bounded by ~0.5 LSB.
    tol = 0.75 * one_lsb  # generous bound around the 0.5-LSB quantization worst case

    print(f"samples compared : {n}")
    print(f"FS (2^(n-1)-1)   : {fs}  (1 LSB = {one_lsb:.6e})")
    print(f"max abs error    : {max_abs:.6e}  ({max_abs / one_lsb:.3f} LSB)")
    print(f"rms error        : {rms:.6e}")
    print(f"SNR              : {snr_db:.2f} dB")
    if max_abs <= tol:
        print(f"GR-DIFI LOSS OK: recovered within {tol / one_lsb:.2f} LSB "
              f"(export quantization only; gr-difi decode is lossless)")
        return 0
    print(f"GR-DIFI LOSS FAIL: max error {max_abs / one_lsb:.3f} LSB exceeds "
          f"{tol / one_lsb:.2f} LSB tolerance")
    return 1


if __name__ == "__main__":
    sys.exit(main())
