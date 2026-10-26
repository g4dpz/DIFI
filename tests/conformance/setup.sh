#!/usr/bin/env bash
# Set up the DIFI conformance oracle.
#
# The oracle validates our packets against the DIFI Consortium's OWN reference packet
# definitions. Those definitions are third-party code with no explicit license grant,
# so this project does NOT redistribute them. Instead this script fetches them on demand
# (at a pinned commit) into a gitignored local directory, and installs `construct` — the
# only dependency they need — into a venv.
#
# Nothing fetched or created here is committed to this repository.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"

# Pinned DIFI-Certification commit for reproducibility (update deliberately).
DIFI_CERT_REPO="https://raw.githubusercontent.com/DIFI-Consortium/DIFI-Certification"
DIFI_CERT_COMMIT="6ee49d1e83c89bc4c35da7b3dd1e52c2a2d0063b"

# Gitignored local cache of the Consortium's definitions (not committed).
DEFS_DIR="$HERE/.difi-cert/packet_definitions"
VENV="${DIFI_VENV:-/tmp/difi-venv}"

echo "Fetching DIFI Consortium reference definitions (commit ${DIFI_CERT_COMMIT:0:12})..."
mkdir -p "$DEFS_DIR"
: > "$DEFS_DIR/__init__.py"
for f in difi_context_v1_1.py difi_data_v1_1.py difi_version_v1_1.py construct_custom_types.py; do
  url="$DIFI_CERT_REPO/$DIFI_CERT_COMMIT/packet_definitions/$f"
  if ! curl -sfL "$url" -o "$DEFS_DIR/$f"; then
    echo "ERROR: could not fetch $f from $url" >&2
    exit 1
  fi
done
echo "  definitions cached at $DEFS_DIR (gitignored)"

echo "Creating venv and installing 'construct'..."
python3 -m venv "$VENV"
"$VENV/bin/pip" install -q --upgrade pip
"$VENV/bin/pip" install -q construct

echo ""
echo "conformance oracle ready."
echo "run: $VENV/bin/python3 $HERE/certify.py <path-to-golden-dir>"
echo "e.g. $VENV/bin/python3 $HERE/certify.py \$(git -C \"$HERE\" rev-parse --show-toplevel)/tests/golden"
