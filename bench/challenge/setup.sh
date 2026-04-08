#!/bin/bash
# Nebula Challenge Setup
# Converts Go source files to Nebula (.neb) for compact reading.
# The model reads .neb files but writes Go directly to .go files.
#
# Usage:
#   ./setup.sh          # Create .neb files from Go source
#   ./setup.sh clean    # Remove .neb files

set -euo pipefail

CHALLENGE_DIR="$(cd "$(dirname "$0")" && pwd)"
NEBULA_BIN="${NEBULA_BIN:-$(dirname "$CHALLENGE_DIR")/../compiler/target/debug/nebula}"

if [ "${1:-}" = "clean" ]; then
    rm -f "$CHALLENGE_DIR"/*.neb
    echo "Cleaned .neb files."
    exit 0
fi

if [ ! -x "$NEBULA_BIN" ]; then
    echo "Error: Nebula binary not found at: $NEBULA_BIN"
    echo "Build it: cd compiler && cargo build"
    exit 1
fi

echo "Converting Go → Nebula..."
"$NEBULA_BIN" from-go "$CHALLENGE_DIR/main.go" > "$CHALLENGE_DIR/main.neb" 2>/dev/null
"$NEBULA_BIN" from-go "$CHALLENGE_DIR/main_test.go" > "$CHALLENGE_DIR/main_test.neb" 2>/dev/null

GO_TOKENS=""
NEB_TOKENS=""
PYTHON="${CHALLENGE_DIR}/../../.venv/bin/python3"
[ -x "$PYTHON" ] || PYTHON="python3"
if "$PYTHON" -c "import tiktoken" 2>/dev/null; then
    read GO_TOKENS NEB_TOKENS < <("$PYTHON" - "$CHALLENGE_DIR/main.go" "$CHALLENGE_DIR/main.neb" <<'PYEOF'
import sys, tiktoken
enc = tiktoken.get_encoding("cl100k_base")
with open(sys.argv[1]) as f: go_tok = len(enc.encode(f.read()))
with open(sys.argv[2]) as f: neb_tok = len(enc.encode(f.read()))
print(f"{go_tok} {neb_tok}")
PYEOF
    )
fi

GO_LINES=$(wc -l < "$CHALLENGE_DIR/main.go" | tr -d ' ')
NEB_LINES=$(wc -l < "$CHALLENGE_DIR/main.neb" | tr -d ' ')

echo ""
echo "  main.go  → main.neb   ($GO_LINES → $NEB_LINES lines)"
if [ -n "$GO_TOKENS" ]; then
    DELTA=$("$PYTHON" -c "print(f'{($NEB_TOKENS - $GO_TOKENS) * 100.0 / $GO_TOKENS:+.1f}')")
    echo "  Tokens: $GO_TOKENS → $NEB_TOKENS (${DELTA}%)"
fi
echo ""
echo "Ready. The model should read .neb files and write to .go files."
