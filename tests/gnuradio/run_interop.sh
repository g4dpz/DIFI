#!/usr/bin/env bash
# GNU Radio <-> difi-interop interop test (Route 1).
#
# Puts GNU Radio's runtime and its UDP block (network.socket_pdu) in the path, in both
# directions, against each of our implementations:
#   ours(export) -> GNU Radio(gr_receive)   [rust and cpp]
#   GNU Radio(gr_send) -> ours(import-interop) [rust and cpp]
#
# The DIFI packet logic on the GNU Radio side is the DIFI Consortium reference
# (construct definitions). This is Route 1; gr-difi (its own DIFI blocks) is Route 2.
#
# Interop is judged within one quantisation step (1 LSB): a peer whose float math differs
# by a ULP can map an isolated sample to the adjacent code, which is still correct.
#
# Requires GNU Radio's python (auto-detected) with `construct` installed, and both tools
# built. Skips cleanly (exit 0, message) if GNU Radio is unavailable.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HERE="$ROOT/tests/gnuradio"
RUST_TOOL="$ROOT/rust/target/debug/difi-tool"
CPP_TOOL="$ROOT/cpp/build/difi-tool"

# Locate a GNU Radio python.
GRPY="${DIFI_GR_PYTHON:-}"
if [ -z "$GRPY" ]; then
  GRPY="$(ls /opt/homebrew/Cellar/gnuradio/*/libexec/venv/bin/python 2>/dev/null | head -1)"
fi
if [ -z "$GRPY" ] || ! "$GRPY" -c "import gnuradio, construct" 2>/dev/null; then
  echo "SKIP: GNU Radio python with 'construct' not found."
  echo "  (set DIFI_GR_PYTHON, and: <gr-python> -m pip install construct)"
  exit 0
fi
[ -x "$RUST_TOOL" ] || { echo "build rust first: cd rust && cargo build --bin difi-tool"; exit 1; }
[ -x "$CPP_TOOL" ]  || { echo "build cpp first: cmake -S cpp -B cpp/build && cmake --build cpp/build"; exit 1; }

port=53100
fail=0

ours_to_gr() {   # $1=label $2=tool
  local label="$1" tool="$2"
  local p=$((port++))
  local out
  out="$("$GRPY" "$HERE/gr_receive.py" "$p" 16 & grp=$!
         sleep 1.0
         "$tool" export "127.0.0.1:$p" >/dev/null 2>&1
         wait $grp)"
  if echo "$out" | grep -q "GR RECV OK"; then
    echo "  ok  $label(export) -> GNU Radio: $(echo "$out" | grep max_abs)"
  else
    echo "FAIL: $label(export) -> GNU Radio"; echo "$out"; fail=1
  fi
}

gr_to_ours() {   # $1=label $2=tool
  local label="$1" tool="$2"
  local p=$((port++))
  local out
  out="$("$tool" import-interop "127.0.0.1:$p" & imp=$!
         sleep 0.6
         "$GRPY" "$HERE/gr_send.py" "$p" >/dev/null 2>&1
         wait $imp)"
  if echo "$out" | grep -q "IMPORT OK"; then
    echo "  ok  GNU Radio -> $label(import): $(echo "$out" | grep 'end-to-end')"
  else
    echo "FAIL: GNU Radio -> $label(import)"; echo "$out"; fail=1
  fi
}

echo "== GNU Radio <-> difi-interop (Route 1: socket_pdu + reference defs) =="
ours_to_gr "rust" "$RUST_TOOL"
ours_to_gr "cpp"  "$CPP_TOOL"
gr_to_ours "rust" "$RUST_TOOL"
gr_to_ours "cpp"  "$CPP_TOOL"

if [ "$fail" -eq 0 ]; then
  echo "GNU RADIO INTEROP: all directions pass"
else
  echo "GNU RADIO INTEROP: FAILURES above"
fi
exit $fail
