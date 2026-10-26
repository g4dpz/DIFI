#!/usr/bin/env bash
# Cross-implementation interop check.
#
# Proves the two independent implementations agree on the DIFI 1.1 wire format:
#   1. Each tool regenerates the golden packets; all six outputs must be byte-identical
#      to the committed shared oracle in tests/golden/.
#   2. Each tool decodes the OTHER tool's packets and must report identical fields.
#
# Requires both binaries to be built first:
#   cd rust && cargo build --bin difi-tool
#   cd cpp  && cmake -S . -B build && cmake --build build
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUST_TOOL="$ROOT/rust/target/debug/difi-tool"
CPP_TOOL="$ROOT/cpp/build/difi-tool"
GOLDEN="$ROOT/tests/golden"

[ -x "$RUST_TOOL" ] || { echo "missing $RUST_TOOL (run: cd rust && cargo build --bin difi-tool)"; exit 1; }
[ -x "$CPP_TOOL" ]  || { echo "missing $CPP_TOOL (run: cd cpp && cmake -S . -B build && cmake --build build)"; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

FILES=(signal_context_16bit.difi.bin version_context.difi.bin signal_data_16bit.difi.bin)

echo "== 1. byte-identical generation =="
"$RUST_TOOL" gen-golden "$TMP/rust" >/dev/null
"$CPP_TOOL"  gen-golden "$TMP/cpp"  >/dev/null
for f in "${FILES[@]}"; do
  cmp "$TMP/rust/$f" "$GOLDEN/$f" || { echo "FAIL: rust $f != committed oracle"; exit 1; }
  cmp "$TMP/cpp/$f"  "$GOLDEN/$f" || { echo "FAIL: cpp  $f != committed oracle"; exit 1; }
  echo "  ok  $f  (rust == cpp == oracle)"
done

echo "== 2. cross-decode: each tool reads the other's bytes =="
for f in "${FILES[@]}"; do
  # Rust decodes the C++-produced bytes; C++ decodes the Rust-produced bytes.
  r_reads_c="$("$RUST_TOOL" dump "$TMP/cpp/$f")"
  c_reads_r="$("$CPP_TOOL"  dump "$TMP/rust/$f")"
  if [ "$r_reads_c" != "$c_reads_r" ]; then
    echo "FAIL: cross-decode field mismatch on $f"
    diff <(echo "$r_reads_c") <(echo "$c_reads_r") || true
    exit 1
  fi
  echo "  ok  $f  (rust-reads-cpp == cpp-reads-rust)"
done

echo "== 3. live UDP interop (all four directions) =="
COUNT=8
PORT_BASE=54991

# run_case <label> <recv_cmd...> -- <send_cmd...>
run_case() {
  local label="$1"; shift
  local port="$1"; shift
  local recv_tool="$1"; shift
  local send_tool="$1"; shift

  local out
  out="$("$recv_tool" recv "127.0.0.1:$port" "$COUNT" & recv_pid=$!
         sleep 0.3
         "$send_tool" send "127.0.0.1:$port" "$COUNT" >/dev/null
         wait $recv_pid)"
  if [ "$out" = "RECV OK $COUNT" ]; then
    echo "  ok  $label  ($out)"
  else
    echo "FAIL: $label — receiver reported: '$out'"
    exit 1
  fi
}

run_case "rust -> rust" $((PORT_BASE + 0)) "$RUST_TOOL" "$RUST_TOOL"
run_case "cpp  -> cpp"  $((PORT_BASE + 1)) "$CPP_TOOL"  "$CPP_TOOL"
run_case "cpp  -> rust" $((PORT_BASE + 2)) "$RUST_TOOL" "$CPP_TOOL"
run_case "rust -> cpp"  $((PORT_BASE + 3)) "$CPP_TOOL"  "$RUST_TOOL"

echo "== 4. sample-mapper agreement (float -> DIFI integer) =="
for d in 4 8 12 16; do
  if diff <("$RUST_TOOL" map-check "$d") <("$CPP_TOOL" map-check "$d") >/dev/null; then
    echo "  ok  bit_depth=$d  (rust codes == cpp codes, identical loss)"
  else
    echo "FAIL: mapper disagreement at bit_depth=$d"
    diff <("$RUST_TOOL" map-check "$d") <("$CPP_TOOL" map-check "$d") || true
    exit 1
  fi
done

echo "== 5. generated reference signal agrees across implementations =="
if diff <("$RUST_TOOL" gen-check) <("$CPP_TOOL" gen-check) >/dev/null; then
  echo "  ok  generated + mapped DIFI codes are bit-identical (rust == cpp)"
else
  echo "FAIL: generated signal differs between implementations"
  exit 1
fi

echo "== 6. end-to-end signal export/import over DIFI (Req 7.1) =="
E2E_PORT_BASE=55300
run_e2e() {
  local label="$1"; shift
  local port="$1"; shift
  local imp="$1"; shift
  local exp="$1"; shift
  local out
  out="$("$imp" import "127.0.0.1:$port" & imp_pid=$!
         sleep 0.6
         "$exp" export "127.0.0.1:$port" >/dev/null 2>&1
         wait $imp_pid)"
  if echo "$out" | grep -q "IMPORT OK"; then
    echo "  ok  $label  (recovered within documented quantization loss)"
  else
    echo "FAIL: $label export/import"
    echo "$out"
    exit 1
  fi
}
run_e2e "rust -> rust" $((E2E_PORT_BASE + 0)) "$RUST_TOOL" "$RUST_TOOL"
run_e2e "cpp  -> cpp"  $((E2E_PORT_BASE + 1)) "$CPP_TOOL"  "$CPP_TOOL"
run_e2e "cpp  -> rust" $((E2E_PORT_BASE + 2)) "$RUST_TOOL" "$CPP_TOOL"
run_e2e "rust -> cpp"  $((E2E_PORT_BASE + 3)) "$CPP_TOOL"  "$RUST_TOOL"

echo "== 7. reproducible signal + manifest (Req 6.1, 6.2) =="
MAN_WORK="$TMP/manwork"
# Each tool regenerates the raw signal; both must produce the same SHA-256, and each
# must verify the committed manifest.
r_sha="$("$RUST_TOOL" generate "$MAN_WORK/rust" 2>/dev/null | grep sha256 | awk '{print $2}')"
c_sha="$("$CPP_TOOL"  generate "$MAN_WORK/cpp"  2>/dev/null | grep sha256 | awk '{print $2}')"
if [ -n "$r_sha" ] && [ "$r_sha" = "$c_sha" ]; then
  echo "  ok  generated raw cf32 SHA-256 identical (rust == cpp): ${r_sha:0:16}..."
else
  echo "FAIL: manifest SHA mismatch (rust=$r_sha cpp=$c_sha)"; exit 1
fi
MAN="$ROOT/signals/afs-style-tone-4096.manifest.json"
if [ -f "$MAN" ]; then
  for tl in "rust:$RUST_TOOL" "cpp:$CPP_TOOL"; do
    l="${tl%%:*}"; t="${tl#*:}"
    if "$t" verify-manifest "$MAN" >/dev/null 2>&1; then
      echo "  ok  $l regenerates + verifies committed manifest SHA-256"
    else
      echo "FAIL: $l could not verify committed manifest"; exit 1
    fi
  done
else
  echo "  note: no committed manifest at signals/ (run: difi-tool generate work)"
fi
# The raw signal must never be committable.
( cd "$ROOT" && git check-ignore work/x.cf32 >/dev/null 2>&1 ) \
  && echo "  ok  raw I/Q working dir is gitignored (artifact policy)" \
  || echo "  warn: work/ not gitignored"

echo "== 8. DIFI Consortium conformance (external oracle) =="
VENV_PY="${DIFI_VENV_PY:-/tmp/difi-venv/bin/python3}"
if [ -x "$VENV_PY" ] && "$VENV_PY" -c "import construct" 2>/dev/null; then
  for tool_label in "rust:$RUST_TOOL" "cpp:$CPP_TOOL"; do
    label="${tool_label%%:*}"; tool="${tool_label#*:}"
    gdir="$TMP/conf-$label"
    "$tool" gen-golden "$gdir" >/dev/null
    if "$VENV_PY" "$ROOT/tests/conformance/certify.py" "$gdir" >/dev/null 2>&1; then
      echo "  ok  $label packets pass the DIFI Consortium reference validation"
    else
      echo "FAIL: $label packets rejected by DIFI conformance oracle"
      "$VENV_PY" "$ROOT/tests/conformance/certify.py" "$gdir" || true
      exit 1
    fi
  done
else
  echo "  skip (conformance venv not found; run tests/conformance/setup.sh)"
fi

echo "ALL INTEROP CHECKS PASSED"
