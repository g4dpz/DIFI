# DIFI & VITA 49.2 — Primer and Reference

A condensed orientation for the DIFI I/Q interoperability work. Written to get an
engineer new to DIFI/VITA 49.2 productive quickly, and to serve as the standing
reference for the spec in `.kiro/specs/difi-iq-interop/`.

> Sourcing note: this is paraphrased and summarised from the public sources linked
> below (primarily the DIFI Consortium's own materials). It is not a substitute for
> the official IEEE-ISTO Std 4900-2021 — for anything load-bearing, consult the
> official standard. Content was rephrased for compliance with licensing restrictions.

---

## 1. The 60-second mental model

- **I/Q samples** — a complex baseband representation of a radio signal (In-phase +
  Quadrature). Digitizers produce them; modems/signal processors consume them.
- **VITA 49.2 (a.k.a. VRT, VITA Radio Transport)** — an IEEE/VITA standard for
  packetising timestamped I/Q data plus metadata. It is deliberately very flexible —
  so flexible that two conformant implementations may still not interoperate.
- **DIFI (Digital IF Interoperability)** — a tightly constrained *profile* (schema) of
  VITA 49.2, carried over UDP/IP. It removes most of VITA 49.2's optionality so that a
  digitizer from one vendor can feed a modem/application from another without a
  proprietary interface. The constraint is the whole point.
- **Why ESA/ESTRACK cares** — a standard digital IF interface lets an all-digital
  ground station mix digitizers, modems and software from any vendor, and enables
  hardware-in-the-loop by streaming the same DIFI I/Q to/from real equipment.

Formal name of the DIFI standard: **IEEE-ISTO Std 4900-2021: Digital IF
Interoperability Standard**.

---

## 2. Where to learn it (authoritative + useful, in reading order)

1. **DIFI 101 Tutorial** — the best on-ramp. Explains DIFI from scratch, shows the
   packet byte layout, and gives runnable Python (`difi_tx.py`) to build and parse a
   real DIFI packet with only numpy, plus a Wireshark dissector.
   https://github.com/DIFI-Consortium/DIFI-Certification/blob/main/DIFI_101_Tutorial.md
2. **DIFI Consortium site** — official home: standard downloads, slide decks, news.
   https://dificonsortium.org/
3. **DIFI-Certification repo** — reference generate/parse code (`certify_sink.py`,
   `certify_source.py`) and the Wireshark dissector.
   https://github.com/DIFI-Consortium/DIFI-Certification
4. **gr-difi** — GNU Radio source/sink blocks; shows DIFI inside a real DSP flowgraph
   (directly relevant to the HITL / ESTRACK angle).
   https://github.com/DIFI-Consortium/gr-difi
5. **PySDR** — free primer on I/Q, complex baseband and sampling if the DSP
   foundations need shoring up. https://pysdr.org
6. **"VITA 49 and DIFI Digital IF Explained"** — readable operator/engineer overview.
   https://satellitegroundstation.com/resources/vita-49-and-difi-digital-if-explained/

### The specs (primary sources)
- **DIFI: IEEE-ISTO Std 4900-2021.** Current version **1.2.1** (approved Feb 2025);
  **1.1** (2022) is the simpler profile. Download from dificonsortium.org. (An approved
  1.2.1 PDF has been mirrored publicly, but prefer the official copy.)
- **VITA 49.2 (VRT)** — the parent standard, published by VITA (https://vita.com). It is
  a **paid** standard (unlike the free CCSDS books). For implementing DIFI you generally
  do **not** need to buy VITA 49.2 — the DIFI documents cover the constrained subset you
  actually build to.

### Recommended first hour
Read the DIFI 101 tutorial and **run its `difi_tx.py`**, capture the packet in Wireshark
with the DIFI dissector, then run the parser. Seeing one real packet end-to-end teaches
more than reading the spec cold. Then skim the 1.2.1 spec for the context-packet fields.

---

## 3. Packet structure (DIFI over UDP)

DIFI defines the **payload of UDP packets**. UDP payload size is adjustable (≈128 to
9000 octets), which sets how many I/Q samples fit per packet.

**Three packet types:**
- **Signal Data** — the actual I/Q samples, with an integer + fractional-seconds
  timestamp marking the precise time of the first sample.
- **Signal Context** — metadata: timestamp, bandwidth, RF frequency, gain, sample rate,
  and the **data format** used in the data packets. Sent periodically or on change.
- **Version Context** — software/firmware version and the DIFI spec version in use. Sent
  periodically (e.g. once per second).

**Signal Data packet header** (32-bit words, big-endian), per the DIFI 1.1 profile — the
first word packs: Packet Type, Class-ID present bit, indicators, TSI (integer-timestamp
type), TSF (fractional-timestamp type), a 4-bit **sequence number** (mod-16, for
out-of-order detection), and Packet Size (in 32-bit words). Then:
- **Stream ID** (32-bit) — defaults to 0; lets multiple independent streams share a port
  (receiver filters on it).
- **OUI** = `0x6A621E` (the VITA OUI).
- **Information Class Code / Packet Class Code** (16-bit each).
- **Integer-seconds Timestamp** (32-bit).
- **Fractional-seconds Timestamp** (64-bit, picoseconds past the integer second).
- **Signal Data Payload** — the I/Q samples.

The header is **7 words (28 bytes)**; a valid 1.1 signal data packet has Packet Size =
N + 7 words for N payload words.

**Sample format — the detail that matters most for us:**
DIFI mandates **complex Cartesian I/Q as signed integers, bit depth selectable 4–16 bits**
(set in the Signal Context packet). No zero padding — for depths other than 8 and 16,
samples share bytes, and samples-per-packet must produce a whole number of 4-byte words.
For an n-bit signed sample the max magnitude is 2^(n-1) − 1.

> **Implication for this project:** the ESA competition reference signals were **float32**
> (e.g. LSIS-AFS IQ32). DIFI is **integer**. Adopting DIFI therefore forces a
> **quantization + scaling decision** (bit depth, reference level, gain) — this is not a
> free wrapper. The DIFI Consortium's MILCOM "Reference Gain and Scaling" material covers
> exactly this; it is the resource to consult when we pin the sample-format mapping.

---

## 4. DIFI 1.1 vs 1.2.1 (which to target)

- **1.1 (2022)** — the simpler profile: Signal Data, Signal Context, Version Context.
  Enough for a first I/Q interoperability exchange.
- **1.2.0 / 1.2.1** — adds a standard **flow-control** mechanism and **command** packets,
  via a new Information Class (0x0002) and new Packet Classes (0x0002, 0x0003, 0x0005,
  0x0006), improving sync and flow control.

**Working recommendation:** target **1.1 first** for the initial interop exchange, with the
design leaving room to adopt 1.2.1 when flow control and HITL timing make it worthwhile.
(ESTRACK's expectations may override this — open question for the team.)

### A clarifier on 1.2.1 terminology (easy to trip over)
- **Packet Class** tells you how to parse the bits of that packet.
- **Information Class** tells you what other streams to expect alongside it and what the
  system is doing.
Reading the 1.2.1 spec, the apparent complexity is largely naming: data packets differ
only in whether time is in seconds vs samples; context classes are near-identical except
the version packet; command packets differ only in fractional-timestamp interpretation.

---

## 5. Tooling we can build on (so we don't start from zero)

- **gr-difi** — GNU Radio DIFI Source/Sink (UDP or TCP).
- **DIFI-Certification Python** — `certify_sink.py` (generate) / `certify_source.py`
  (parse); good reference for field-level correctness.
- **Wireshark DIFI dissector** — `.lua` plugin in the DIFI-Certification repo; invaluable
  for debugging the wire format.
- **Vendor hardware** — e.g. Kratos SpectralNet digitizers speak DIFI; USRP/UHD support is
  emerging — relevant for the eventual ESTRACK hardware-in-the-loop step.

---

## 6. How this connects to the prototyping effort

- The hub (**esa/CCSDS-Community-Prototyping**) coordinates independent implementations and
  shared interop resources; it is intentionally not one monorepo.
- DIFI is proposed (by Clemens) as **the** I/Q interchange format across implementations,
  with hardware-in-the-loop via ESTRACK as the second step.
- This project builds a **DIFI I/Q interoperability toolkit** — a reference source/sink,
  small golden packets, and validation tooling — that implementations (LunaLink, a future
  SCSC reference, ESTRACK hardware) can exchange I/Q through.
- **Repository-size discipline** (see the artifact-policy steering doc): large I/Q never
  goes into Git; we commit generators + manifests + small golden packets, and host large
  canonical datasets externally by checksum.

---

## 7. Open questions to resolve with the team
1. **DIFI version** — 1.1 first, or straight to 1.2.1 (what will ESTRACK expect)?
2. **Sample format mapping** — bit depth and scaling/reference-level convention to map the
   existing float32 reference signals into DIFI integer samples.
3. **Canonical dataset hosting** — Zenodo (DOIs) vs GitHub Release assets for the few large
   fixed vectors.
4. **Transport** — UDP only (per DIFI), or also TCP (as gr-difi allows) for lossless
   file-like exchange during development?
