# DIFI I/Q Interoperability Toolkit

A reference **DIFI (Digital IF Interoperability)** I/Q exchange layer for CCSDS
community prototyping — so independent reference implementations (and, later, ESTRACK
hardware) can exchange digitized I/Q over a single standard interface.

DIFI is a tightly constrained profile of **VITA 49.2 (VRT)** over UDP/IP
(IEEE-ISTO Std 4900-2021). Adopting it as the common I/Q interchange format removes
proprietary interfaces between digitizers, modems and signal-processing software, and
opens a clean path to **hardware-in-the-loop** testing with ESA's all-digital ground
station (ESTRACK).

## Context

- Part of the CCSDS community prototyping effort coordinated at
  **[esa/CCSDS-Community-Prototyping](https://github.com/esa/CCSDS-Community-Prototyping)**
  — a hub of *independent* implementations that test together (not a single monorepo).
- Proposed direction: use DIFI as **the** interoperability exchange format across
  implementations.

## What this project provides (planned)

- A reference **DIFI source/sink** (encode/decode DIFI data, context and version packets).
- A small set of **golden packets** validated against the DIFI certification tooling.
- **Signal generators + manifests** so large I/Q vectors are reproducible, never stored
  raw in Git (see the artifact policy below).
- A validation/interop harness other implementations can check against.

## Structure

| Path | Purpose |
|------|---------|
| `.kiro/specs/difi-iq-interop/` | The spec: `requirements.md`, `design.md`, `tasks.md` |
| `.kiro/steering/artifact-policy.md` | Mandatory repo-size / artifact rules |
| `docs/reference/DIFI-VITA49-PRIMER.md` | Condensed DIFI + VITA 49.2 primer and source list |

## Start here

- New to DIFI/VITA 49.2? Read **`docs/reference/DIFI-VITA49-PRIMER.md`** first.
- The work plan lives in the spec under `.kiro/specs/difi-iq-interop/`.

## Repository-size discipline (important)

Large raw I/Q is **never** committed to Git. We commit generators + manifests + small
golden packets, and host large canonical datasets externally (Zenodo/Release assets) by
checksum. See `.kiro/steering/artifact-policy.md`. This keeps clones small no matter how
many signals accumulate.

## Licence

TBD (intended to be open, compatible with contribution to the ESA community hub).
