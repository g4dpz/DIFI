# Reference Signals — Manifests (spec task 11 / Req 6)

This directory holds **manifests**, not raw I/Q. Large raw signals are never committed
(see `.kiro/steering/artifact-policy.md`); instead, each reference signal is reproduced
on demand from a deterministic generator plus the small committed manifest here.

## What a manifest records (Req 6.2)

Each `*.manifest.json` captures everything needed to reproduce and verify the signal:

- `sha256` + `byte_size` — checksum and size of the generated raw file.
- `raw_format` — `cf32` (interleaved little-endian float32 I/Q).
- `params` — the exact generator parameters (sample count, sample rate, tones, peak).
- `difi` — the DIFI context the signal is exported with (bit depth, full-scale, RF
  frequency, bandwidth, sample rate, gain).
- `regen_command` — the exact command that regenerates the raw file.

## Generate and verify

```sh
# Regenerate the raw I/Q into the gitignored work/ dir and (re)write the manifest here.
difi-tool generate work            # Rust or C++ — byte-identical output

# Verify a committed manifest: regenerate and confirm the SHA-256 matches.
difi-tool verify-manifest signals/afs-style-tone-4096.manifest.json
```

Both the Rust and C++ tools produce **byte-identical** raw output (same SHA-256) and
write identical manifests, so a manifest is a cross-implementation reproduction oracle:
either implementation regenerates exactly the same bytes.

## Current signals

| Manifest | Samples | Format | Notes |
|----------|---------|--------|-------|
| `afs-style-tone-4096.manifest.json` | 4096 | cf32 | Reproducible LSIS-AFS-*style* waveform (tone sum + envelope). Used by the export/import interop evidence. |

The raw output lands in `work/` (gitignored). The `afs-style-tone-4096` signal is a
representative stand-in, not a real LSIS-AFS vector.

## Hosting decision (DQ-4 / Req 6.4 — resolved)

The toolkit's committed reference is this **reproducible, generator-backed** waveform; it
needs no external host because it is regenerated identically on demand. External hosting
by SHA-256 is the **documented, ready mechanism** for a real canonical vector when one is
contributed — see `docs/DATASET-HOSTING-DECISION.md` for the decision, rationale, and the
exact procedure (compute SHA-256 → host on Zenodo or a Release asset → add a manifest
recording the checksum + URL → consumers verify against the SHA-256). Each manifest already
records `sha256` + `byte_size`, which is the binding hosting contract; adding a real vector
is purely additive.
