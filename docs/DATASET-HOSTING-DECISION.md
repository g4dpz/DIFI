# Decision Record: Canonical Dataset Hosting (DQ-4 / Req 6.4)

**Status:** Resolved (`difi-1.2-dataplane` task 10).
**Supersedes:** DQ-4 "OPEN" in `.kiro/specs/difi-iq-interop/design.md`, and the Req 6.4
"partial" note in that feature's requirements.

## Question

DIFI I/Q Interop Req 6.4 / design DQ-4: large canonical I/Q must not be committed to Git;
the manifest records a SHA-256 so large data *can* be hosted externally (Zenodo vs GitHub
Release assets). The open question was whether to **(A)** wire up a real externally-hosted
canonical dataset now, or **(B)** formally adopt the reproducible generator-backed waveform
as the committed reference, with external-by-checksum hosting documented and ready for when
a real vector is contributed.

## Decision

**Option B.** The toolkit's committed reference signal is the **reproducible,
generator-backed** waveform `afs-style-tone-4096` (deterministic tone-sum + envelope,
regenerated identically by both implementations from committed parameters). External
hosting by SHA-256 is the **documented, ready mechanism** for a real canonical vector, not
wired up now because no real vector is in hand.

## Rationale

1. **No real canonical vector is available or needed yet.** The only reference the toolkit
   requires today is a representative waveform to drive the export/import and cross-decode
   interop evidence. The generator-backed signal does that fully: both implementations
   regenerate byte-identical `cf32` (verified SHA-256 `baa9144…`, interop.sh §7), and the
   export/import round-trip recovers it within the documented quantization loss.
2. **The artifact policy prefers reproduction over storage.** `.kiro/steering/
   artifact-policy.md` mandates generators + manifests + small golden packets, with large
   canonical data hosted externally *only when a real dataset must be preserved as-is*.
   Inventing an external host for a signal that is fully reproducible would add a network
   dependency and a large binary for no benefit — the opposite of the policy's intent.
3. **Hosting a real LSIS-AFS vector has provenance and licensing questions** that are out
   of scope for this toolkit and would need to be resolved by whoever contributes the
   vector, not pre-empted here.
4. **The external-by-checksum contract already exists.** Each manifest records `sha256`
   and `byte_size` — which *is* the hosting contract. Adding a real vector is purely
   additive: a new manifest, the large file hosted externally, and its SHA-256 recorded.

## The external-hosting procedure (for a future real vector)

When a real canonical vector (e.g. an actual LSIS-AFS IQ32 capture) is contributed:

1. Compute its SHA-256 and byte size.
2. Host the raw file **externally** — Zenodo (preferred: gives a citable DOI and long-term
   archival) or a GitHub Release asset (lighter weight, no DOI). Do **not** commit it.
3. Add a manifest under `signals/` recording: `name`, `raw_format`, `byte_size`, `sha256`,
   the DIFI context it is exported with, and — for a non-reproducible capture — the
   external URL and provenance in the `regen_command`/notes field (a capture has no
   generator, so the "regeneration" step is "download from <URL> and verify SHA-256").
4. A consumer fetches the file from the URL and verifies it against the recorded SHA-256
   before use. The checksum is the integrity + identity guarantee; the host is
   interchangeable.

Zenodo vs Release asset: **Zenodo** when the vector is a citable reference dataset meant to
outlive the repo; a **Release asset** when it is a convenience copy tied to a toolkit
version. Either satisfies Req 6.4 — the SHA-256 in the manifest is the binding contract.

## What remains true (honest limitations)

- The committed `afs-style-tone-4096` is a **representative stand-in**, not a real
  LSIS-AFS vector. All interop evidence that uses it says so.
- No real external dataset is hosted today — by decision, not omission. The mechanism and
  procedure above are in place for when one is contributed.
