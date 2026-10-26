# Conformance Evidence — DIFI Wireshark Dissector (Req 3.1)

**Requirement 3.1:** our encoded packets SHALL parse cleanly in the DIFI Consortium
Wireshark dissector with all fields interpreted correctly.

This is the one conformance criterion the `construct`-based check (`certify.py`, Req
3.2/3.3) does not cover. It is now met — automated, headless, via `tshark`.

## How it works

`tests/conformance/wireshark_check.py`:
1. Generates the golden packets with our `difi-tool`.
2. Wraps each DIFI UDP payload in Ethernet/IP/UDP with `text2pcap -u` → a `.pcap`.
3. Runs `tshark -X lua_script:difi-dissector.lua --disable-protocol vrt -T fields` over
   the pcap and extracts the dissected DIFI fields.
4. Asserts each packet is recognised as DIFI (OUI `0x6A621E`) with the expected packet
   type, stream id, and packet class.

The DIFI dissector (`difi-dissector.lua`, fetched on demand by `setup.sh`, pinned, not
vendored) registers as a **UDP heuristic** keyed on the OUI, so the UDP port is
irrelevant. Wireshark's built-in VITA 49 dissector (`vrt`) also claims DIFI-shaped UDP
and would win the heuristic race, so it is disabled with `--disable-protocol vrt`.

## Setup

`tshark` + `text2pcap` from Wireshark. On macOS they live in the app bundle at
`/Applications/Wireshark.app/Contents/MacOS/` (auto-detected; override with
`DIFI_TSHARK` / `DIFI_TEXT2PCAP`). Verified with **TShark 4.6.9** (Lua 5.4 enabled).

```sh
tests/conformance/setup.sh      # fetches the dissector (and the construct defs)
python3 tests/conformance/wireshark_check.py tests/golden
```

## Result — all three packet types dissect with correct field values

```
[PASS] signal_context_16bit.difi.bin: type=0x04 stream=305419896 — 9 decoded fields match (class=0x0001)
[PASS] version_context.difi.bin:      type=0x05 stream=305419896 — 8 decoded fields match (class=0x0004)
[PASS] signal_data_16bit.difi.bin:    type=0x01 stream=305419896 — 4 decoded fields match (class=0x0000)
WIRESHARK DISSECTOR: all golden packets dissect with correct field values
```

The check asserts the **decoded values** the dissector reports, not just recognition:

- **Signal Context:** CIF (`0xFBB98000`), ref point (`0x64`), RF frequency
  (`2 401 000 000` Hz), bandwidth (`1 000 000` Hz), sample rate (`1 250 000` Hz), gain
  (`-3.5` dB), integer + fractional timestamps.
- **Version Context:** CIF0 (`0x80000002`), CIF1 (`0x0000000C`), V49 spec (`0x00000004`),
  year (raw `21` = years-since-2000, i.e. 2021), day (`100`), revision (`1`),
  ICD version (`0`).
- **Signal Data:** sequence number (`0`), integer + fractional timestamps.

`interop.sh` §8b runs this for both the Rust and C++ tools' output; it skips cleanly if
`tshark` or the dissector is absent.

## Notes

- The dissector's `difi.year` field is years-since-2000 (7-bit) and is reported **raw**,
  so 2021 → `21`; our encoder writes `21`, which is what the check asserts. (Finding
  surfaced while extending field coverage.)
- Our gain (`-3.5` dB) lands at the offset the dissector labels `stage2_gain` — the
  dissector splits the level/gain 32-bit word differently from how we name the sub-field.
  The decoded value is correct; only the label differs.
- `text2pcap` synthesises placeholder Ethernet/IP/UDP headers; only the DIFI payload
  (our bytes) is under test.
- Field-value coverage here complements the `construct` conformance check (Req 3.2),
  which validates the complete field set against the reference definitions.
