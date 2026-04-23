#!/usr/bin/env bash
# PreToolUse hook for Write. Denies on supported extensions, redirecting
# to write_code. Non-source files pass through.
set -euo pipefail

input=$(cat)

# Extract the file_path via Python reading stdin (avoids escape-sequence
# bugs in triple-quoted-string interpolation). If the input is malformed,
# Python errors → `|| true` swallows → path stays empty → `allow`.
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
    "permissionDecisionReason": "Nebula plugin: use `write_code` (MCP) instead of built-in Write for source files. Pass Nebula IL as `code`; the plugin translates to the file's native language via its deterministic rule-based pipeline. Validate your Nebula first with `check_nebula(code=...)` if unsure, or dry-run the output with `preview_translation(code=..., target_language=...)`. Retry with mcp__plugin_nebula_nebula__write_code(path=..., code=<Nebula IL>)."
  }
}
EOF
else
    echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}'
fi
