# Steering: Artifact & Repository-Size Policy

**Status:** Always-active project rule.

The thing that bloats CCSDS prototyping repositories is **not code — it is I/Q signal
data and test vectors.** Competition reference signals ran 98 MB to 982 MB each; DIFI
HITL recordings may be larger. Git stores every version of every binary forever, so a
repo can reach tens of GB even when the working tree looks small, and every clone must
download that whole history.

This project is designed so a clone stays small and fast regardless of how many signals
or implementations the ecosystem accumulates. The following rules are mandatory.

## Rules

1. **Never commit raw I/Q larger than ~1 MB into Git history.** If a `.iq`, `.iq32`,
   `.lsisiq`, `.difi` or similar binary exceeds ~1 MB, it must not be committed.

2. **Commit generators, not big outputs.** For any reference signal, commit the
   generator (code + DIFI parameters that deterministically reproduce the bytes) and a
   **manifest** recording: expected SHA-256, byte size, sample format (bit depth,
   scaling), DIFI context parameters, and the exact regeneration command.

3. **Small golden artifacts only.** A handful of representative DIFI packets of a few KB
   each — enough for unit tests and to pin the wire format — may be committed. DIFI's
   4–16 bit integer samples keep these small; short captures suffice to prove interop.

4. **Host large canonical datasets externally.** The few large fixed reference datasets
   that genuinely must be preserved as-is go to **Zenodo** (preferred — gives a citeable
   DOI) or **GitHub Release assets**, referenced from the manifest by SHA-256. Not Git,
   not Git LFS (GitHub LFS quotas make it unattractive at multi-team scale).

5. **`.gitignore` enforces this.** Raw I/Q patterns are gitignored by default; committing
   one requires a deliberate `git add -f`, which should be rare and justified.

## Rationale in one line
Generators + manifests + small golden packets in Git; large data regenerated on demand
or fetched externally by checksum. A clone gives you everything needed to build and test
immediately, and the big data only if and when you actually need it.
