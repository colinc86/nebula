#!/usr/bin/env bash
# PreToolUse hook for Read. Denies the call (with a redirect message) when
# the target path is a supported Nebula source extension. Forces the host
# onto read_code for source files, preventing the double-read pattern
# (native Read + neb_read_code of the same file). Non-source files pass
# through unchanged.
set -euo pipefail

# Read the hook JSON input.
input=$(cat)

# Extract the file_path from tool_input.
path=$(printf %s "$input" | python3 -c "
import json, sys
d = json.loads(sys.stdin.read())
ti = d.get('tool_input') or {}
print(ti.get('file_path') or ti.get('path') or '')
" 2>/dev/null || true)

is_neb=0
case "$path" in
    *.py|*.js|*.ts|*.go|*.swift|*.kt|*.rs|*.c) is_neb=1 ;;
esac

if [ "$is_neb" -eq 1 ]; then
    cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Nebula plugin: use `read_code` (MCP) for source files in supported languages — it returns the file in Nebula IL which is ~16% smaller than native. Retry with mcp__plugin_nebula_nebula__read_code(path=..., offset?, limit?). If you genuinely need raw native bytes (e.g. to construct a byte-exact patch_code call), use mcp__plugin_nebula_nebula__read_native — first-class escape hatch, no IL conversion."
  }
}
EOF
else
    echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}'
fi
