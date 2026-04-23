#!/usr/bin/env bash
# PreToolUse hook for Edit. Denies on supported extensions, redirecting
# to patch_code (native-syntax byte-level edit) or write_code (full-file
# Nebula-accepting write). Non-source files pass through.
set -euo pipefail

input=$(cat)

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
    "permissionDecisionReason": "Nebula plugin: use `write_code` (Nebula-accepting, whole-file) or `patch_code` (byte-level substitution) for source files. Prefer write_code when rewriting more than a few lines — it accepts Nebula IL and translates to the file's native language on write. Use patch_code for small targeted substitutions; pass `nebula=true` if you prefer authoring old_str/new_str as Nebula rather than exact native bytes. Validate your Nebula first with `check_nebula(code=...)` if unsure, or dry-run via `preview_translation(code=..., target_language=...)`. Retry with mcp__plugin_nebula_nebula__write_code(path=..., code=...) or mcp__plugin_nebula_nebula__patch_code(path=..., old_str=..., new_str=...)."
  }
}
EOF
else
    echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}'
fi
