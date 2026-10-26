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
representative stand-in, not a real LSIS-AFS vector; a real vector would be added here as
its own manifest, with the large canonical data hosted externally (Zenodo / Release
asset) and referenced by the recorded SHA-256 (Req 6.4).
