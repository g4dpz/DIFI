# Reconciliation: Toolkit Scaling vs DIFI "Reference Gain and Scaling"

**Requirement:** DIFI I/Q Interop Req 4.3 (carried into the `difi-1.2-dataplane` feature as
task 9): the sample-format scaling / reference-level convention must be reconciled against
the DIFI "Reference Gain and Scaling" guidance, and either confirmed consistent or
adjusted, with the outcome recorded.

**Source:** DIFI Consortium, *"Reference Level, Scaling, and Gain Fields in DIFI ISTO
4900 v1.2"* (MILCOM 2023, Annmarie Stanley / Jim Rosenberg). Content below is paraphrased
and summarised for licensing compliance.

## What the DIFI guidance says

1. **Digital full-scale.** For an n-bit signed sample the largest representable positive
   value is `2^(n-1) − 1`, but DIFI/VITA 49 defines digital **full-scale** as
   `R = 2^(n-1)` (e.g. 128 for 8-bit, 32768 for 16-bit). A full-scale digital sinusoid for
   complex Cartesian I/Q lies on the circle `I² + Q² = R²`.
2. **Reference Level** (context word 18, bits 0–15) ties an analog power in dBm at the
   Reference Point to a *full-scale digital sine wave* — Rx direction in VITA 49.2, extended
   by DIFI v1.2 to the Tx direction by analogy.
3. **Scaling** (context word 18, bits 16–31, new in DIFI v1.2) is an **optional** field
   giving the average power of the samples in dBFS; filled with zeros when unused.
4. **Gain 1 / Gain 2** (context word 19) are **generally disregarded** in DIFI and filled
   with zeros at the source.

## What this toolkit does

- **Sample quantization** (`ScalingConvention`, `rust/src/mapper.rs` /
  `cpp/include/difi/difi.hpp`): a float amplitude is normalised so `full_scale` maps to the
  denominator `FS = 2^(n-1) − 1` (the max representable positive code), applied symmetrically
  to I and Q; over-range saturates.
- **Context-packet metadata fields**: Reference Level, Scaling, and both Gain fields are
  emitted as **zeros**.

## Reconciliation outcome

**Confirmed consistent for wire conformance; one deliberate, documented sub-LSB difference
in the application-level quantization reference.**

1. **Context fields (the fields the guidance actually governs): consistent.** Emitting
   Reference Level / Scaling / Gain as zeros is exactly what the guidance prescribes for the
   unused/disregarded case (Gain disregarded → zeros; Scaling optional → zeros if unused).
   Our packets validate clean against the DIFI Consortium reference `construct` definitions
   and dissect correctly in the DIFI Wireshark dissector (`tests/conformance/`), which is
   the authoritative check on these fields.

2. **Quantization full-scale denominator: `2^(n-1) − 1` vs the standard's `2^(n-1)`.** We use
   `2^(n-1) − 1` deliberately:
   - It is **symmetric** for I and Q.
   - Every code it maps onto is **representable**. The standard's `+R` (e.g. `+32768` at
     16-bit) is *not* a representable positive code (the max is `+32767`), so a strict-`R`
     positive full-scale sample must clamp, while `−R = −32768` is representable — an
     asymmetry we avoid.
   - The difference is **sub-LSB and signal-insignificant**: at 16-bit,
     `20·log10(32767/32768) = −0.00027 dB`.
   - `full_scale` is an **application-level** float-amplitude reference for the
     export/import bridge, recorded in the signal manifest — it is *not* a wire field, so it
     cannot affect interoperability of the packets themselves.

3. **For callers needing the standard's exact reference**, `ScalingConvention::
   difi_full_scale_code()` returns `R = 2^(n-1)` so dBFS math can be done against the DIFI
   definition without changing the (symmetric, all-codes-reachable) quantization default.

## Decision

Keep `FS = 2^(n-1) − 1` as the quantization denominator; do **not** change it to `2^(n-1)`.
The change would introduce an I/Q asymmetry and a non-representable positive full-scale for
a −0.00027 dB "gain" in standards-literalness that has no effect on wire conformance (the
context fields, which the guidance governs, are already conformant). The difference is now
documented here and in the mapper source, and the standard's `R` is exposed via
`difi_full_scale_code()`. This closes Req 4.3.
