#!/bin/bash
# Nebula Benchmark Challenge Runner
# Measures: correctness, token compression, timing
#
# Usage:
#   ./run_challenge.sh          # Run full verification
#   ./run_challenge.sh tokens   # Just show token comparison

set -euo pipefail

CHALLENGE_DIR="$(cd "$(dirname "$0")" && pwd)"
NEBULA_BIN="${NEBULA_BIN:-$(dirname "$CHALLENGE_DIR")/../compiler/target/debug/nebula}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${CYAN}=== Nebula Benchmark Challenge ===${NC}"
echo ""

# --- Step 1: Correctness ---
if [ "${1:-}" != "tokens" ]; then
    echo -e "${YELLOW}[1/3] Running Go tests...${NC}"
    cd "$CHALLENGE_DIR"

    TEST_OUTPUT=$(go test -v -count=1 ./... 2>&1) || true

    PASS=$(echo "$TEST_OUTPUT" | grep -c "^--- PASS" || true)
    FAIL=$(echo "$TEST_OUTPUT" | grep -c "^--- FAIL" || true)
    SKIP=$(echo "$TEST_OUTPUT" | grep -c "^--- SKIP" || true)
    TOTAL=$((PASS + FAIL + SKIP))

    echo "  Tests: $PASS pass, $FAIL fail, $SKIP skip (of $TOTAL)"

    if [ "$FAIL" -gt 0 ]; then
        echo -e "  ${RED}FAILING TESTS:${NC}"
        echo "$TEST_OUTPUT" | grep "^--- FAIL" | sed 's/^/    /'
    fi

    # Challenge tests specifically
    CHALLENGE_PASS=$(echo "$TEST_OUTPUT" | grep -c "^--- PASS: TestChallenge" || true)
    CHALLENGE_SKIP=$(echo "$TEST_OUTPUT" | grep -c "^--- SKIP: TestChallenge" || true)
    CHALLENGE_FAIL=$(echo "$TEST_OUTPUT" | grep -c "^--- FAIL: TestChallenge" || true)

    echo ""
    echo -e "  ${CYAN}Challenge features:${NC}"
    echo "    Implemented: $CHALLENGE_PASS/4"
    echo "    Skipped:     $CHALLENGE_SKIP/4"
    if [ "$CHALLENGE_FAIL" -gt 0 ]; then
        echo -e "    ${RED}Failed:      $CHALLENGE_FAIL/4${NC}"
    fi

    # Existing tests must not break
    EXISTING_PASS=$(echo "$TEST_OUTPUT" | grep "^--- PASS" | grep -vc "TestChallenge" || true)
    EXISTING_FAIL=$(echo "$TEST_OUTPUT" | grep "^--- FAIL" | grep -vc "TestChallenge" || true)

    echo ""
    if [ "$EXISTING_FAIL" -gt 0 ]; then
        echo -e "  ${RED}REGRESSION: $EXISTING_FAIL existing tests broken!${NC}"
    else
        echo -e "  ${GREEN}Existing tests: $EXISTING_PASS/16 pass (no regressions)${NC}"
    fi
    echo ""
fi

# --- Step 2: Token comparison ---
echo -e "${YELLOW}[2/3] Token comparison (Go vs Nebula)...${NC}"

if [ ! -x "$NEBULA_BIN" ]; then
    echo -e "  ${RED}Nebula binary not found at: $NEBULA_BIN${NC}"
    echo "  Build it: cd compiler && cargo build"
    echo "  Or set NEBULA_BIN=/path/to/nebula"
    exit 1
fi

# Convert Go to Nebula
NEB_FILE="$CHALLENGE_DIR/.nebula_output.neb"
"$NEBULA_BIN" from-go "$CHALLENGE_DIR/main.go" > "$NEB_FILE" 2>/dev/null

# Count lines and bytes
GO_LINES=$(wc -l < "$CHALLENGE_DIR/main.go" | tr -d ' ')
GO_BYTES=$(wc -c < "$CHALLENGE_DIR/main.go" | tr -d ' ')
NEB_LINES=$(wc -l < "$NEB_FILE" | tr -d ' ')
NEB_BYTES=$(wc -c < "$NEB_FILE" | tr -d ' ')

echo "  Go source:    $GO_LINES lines, $GO_BYTES bytes"
echo "  Nebula output: $NEB_LINES lines, $NEB_BYTES bytes"

LINE_DELTA=$(( (NEB_LINES - GO_LINES) * 100 / GO_LINES ))
BYTE_DELTA=$(( (NEB_BYTES - GO_BYTES) * 100 / GO_BYTES ))

echo "  Line delta:   ${LINE_DELTA}%"
echo "  Byte delta:   ${BYTE_DELTA}%"

# Token count if tiktoken available
if python3 -c "import tiktoken" 2>/dev/null; then
    echo ""
    python3 - "$CHALLENGE_DIR/main.go" "$NEB_FILE" <<'PYEOF'
import sys, tiktoken
enc = tiktoken.get_encoding("cl100k_base")

with open(sys.argv[1]) as f:
    go_src = f.read()
with open(sys.argv[2]) as f:
    neb_src = f.read()

go_tokens = len(enc.encode(go_src))
neb_tokens = len(enc.encode(neb_src))
delta = (neb_tokens - go_tokens) * 100.0 / go_tokens

print(f"  Go tokens:    {go_tokens}")
print(f"  Nebula tokens: {neb_tokens}")
print(f"  Token delta:  {delta:+.1f}%")
if delta < 0:
    print(f"  Savings:      {abs(neb_tokens - go_tokens)} tokens ({abs(delta):.1f}% compression)")
else:
    print(f"  Expansion:    {neb_tokens - go_tokens} tokens ({delta:.1f}% larger)")
PYEOF
else
    echo -e "  ${YELLOW}Install tiktoken for token counts: pip install tiktoken${NC}"
fi

rm -f "$NEB_FILE"

echo ""

# --- Step 3: Summary ---
echo -e "${YELLOW}[3/3] Summary${NC}"
echo "  +--------------------------+--------+"
echo "  | Metric                   | Result |"
echo "  +--------------------------+--------+"
printf "  | %-24s | %5s%% |\n" "Line compression" "$LINE_DELTA"
printf "  | %-24s | %5s%% |\n" "Byte compression" "$BYTE_DELTA"
if [ "${1:-}" != "tokens" ]; then
    printf "  | %-24s |  %s/16 |\n" "Existing tests pass" "$EXISTING_PASS"
    printf "  | %-24s |   %s/4 |\n" "Challenge features" "$CHALLENGE_PASS"
fi
echo "  +--------------------------+--------+"
echo ""
echo -e "${CYAN}Done.${NC}"
