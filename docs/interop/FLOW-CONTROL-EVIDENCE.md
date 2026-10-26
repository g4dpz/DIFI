# Interop Evidence — DIFI 1.2 Flow-Controlled UDP Exchange

**Requirements:** 5.1 (send/receive DIFI over UDP), 5.3 (multiple streams filtered by
Stream ID), and the `difi-1.2-dataplane` feature's Req 5.1/5.3 (Rust↔C++ exchange a short
stream including Timing Flow Control packets; both interpret the buffer state
consistently; result recorded as evidence).

This records a DIFI **1.2** flow-controlled exchange between the two independent
implementations: a Source streams Signal Data while a Sink replies with a **Timing Flow
Control** packet reporting its buffer state, and the Source decodes it — proving the
`0x0005`/`0x0006` control packets interoperate live over UDP, in both directions.

## Setup

| Item | Value |
|------|-------|
| Rust | rustc (workspace toolchain) |
| C++ | Apple clang (C++17) |
| Transport | UDP, 127.0.0.1; data port + separate flow-control port per direction |
| Stream | `COUNT` Signal Data packets (16-bit), Stream ID `0x12345678` |
| Flow control | one Timing Flow Control packet, Packet Class `0x0006` (Real-Time TSF) |

## Pipeline

```
Source:  encode Signal Context + N Signal Data  →  UDP (data port)  →  Sink
Sink:    receive N data packets  →  encode Timing Flow Control (buffer state)
         →  UDP (flow-control port)  →  Source
Source:  decode the flow-control packet  →  print the reported buffer state
Assert:  the Source's decoded flow-control line == the line the Sink reported sending
         (identical fields on both ends)
```

The flow-control packet content is deterministic (a fixed "near-full after the burst"
state standing in for a real buffer-fill policy), so the Source's decoded fields can be
compared exactly against what the Sink encoded, byte-for-byte in meaning across languages.

## Subcommands

```
difi-tool fc-sink   <data-bind> <fc-dest> <count>   # receive data, reply with flow control
difi-tool fc-source <data-dest> <fc-bind> <count>   # stream data, receive + print flow control
```

## Result — all four directions (tests/interop.sh §3c)

| Direction | Outcome |
|-----------|---------|
| rust-src → rust-sink | ok — source decoded the sink's flow control |
| cpp-src  → cpp-sink  | ok — source decoded the sink's flow control |
| rust-src → cpp-sink  | ok — source decoded the sink's flow control |
| cpp-src  → rust-sink | ok — source decoded the sink's flow control |

Decoded flow-control line (identical on both ends, all directions):

```
FC class=0x0006 kind=RealTime msgid=1 controllee=0x000000A1 controller=0x000000B2 \
   fs=1250000 ts_adj=2500 buf=1048576 OF=false NF=true NE=false UF=false
```

Interpretation: the Sink reports a 1 MiB (`1048576`-byte) buffer, **Near-Full** set (the
other fill-level flags clear), a sample rate of 1.25 MHz and a 2500 fs timestamp
adjustment, addressed Controller `0xB2` → Controllee `0xA1`. The Source recovers every
field exactly, in both the same-language and cross-language directions — demonstrating the
two implementations agree on the Timing Flow Control wire format and its buffer-state
semantics.

## Reproduce

```sh
./tests/interop.sh        # section "3c. flow-controlled UDP interop"
```

## Scope / limitations

- The buffer state is a fixed, deterministic report, not the output of a live buffer-fill
  controller. This evidence demonstrates **wire-format and field-semantics interop** for
  the flow-control packet, not a closed-loop pacing algorithm (the pacing *policy* is an
  application concern; the toolkit's job is correct encode/decode of the flow-control
  state, per the design).
- Byte-identity of the flow-control packet itself is additionally asserted by the golden
  oracle (`timing_flow_control_sc/rt.difi.bin`, interop.sh §1).
