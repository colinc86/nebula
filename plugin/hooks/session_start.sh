#!/usr/bin/env bash
# SessionStart hook for the Nebula plugin. Emits a primer on stdout that
# Claude Code injects into the host's system context. v0.2: Nebula-fluent
# host assumed; routing and validation tools called out first, with
# read_native as an honest escape hatch for non-IL work.
set -euo pipefail

cat <<'EOF'
# Nebula plugin — active

Nebula is a compact IL that round-trips across Python, JS, TS, Go, Swift,
Kotlin, Rust, C. This session routes source-file I/O through the plugin's
MCP server; files stay native on disk.

## Critical execution rules

- **NEVER stop after a planning message.** If you write "I'm going to..." or
  "First I'll..." or "Let me..." or similar, your VERY NEXT output MUST be
  a tool call that starts executing. Never end your turn on description.
- **The task is only done when the verify step passes** (e.g. `go test ./...`).
  Keep calling tools (patch_code, write_code, Bash for tests) until it does.
- **If an edit fails, try a different approach.** Do not narrate failure and
  stop — iterate.

## Tools (MCP)

Read / search:
  read_code(path, offset?, limit?)       — file as Nebula IL (raw if ≤40 lines)
  read_native(path, offset?, limit?)     — raw bytes; use for non-source files
                                            or byte-exact patch construction
  search_code(query, path?, glob?)       — grep; Nebula-form snippets

Write / edit:
  write_code(path, code)                 — accepts Nebula, translates on write
  patch_code(path, old_str, new_str,
             replace_all?, glob?,
             nebula?)                    — byte-level replace. Default is
                                            native; pass nebula=true to have
                                            old_str/new_str translated from
                                            Nebula before the match
  translate_code(code, source_language,
                 target_language)        — standalone translation

Validate (cheap, no file I/O):
  check_nebula(code)                     — does this Nebula parse? returns
                                            {ok:true} or {ok:false,line,col,...}
  preview_translation(code,
                      target_language)   — dry-run what write_code would emit

## Recommended flow for source files

1. read_code the file (or a relevant slice).
2. Plan the edit.
3. For a full-file rewrite: emit Nebula, optionally check_nebula the draft,
   optionally preview_translation to eyeball the native form, then write_code.
4. For a targeted edit: patch_code. If authoring the replacement in Nebula
   is easier, pass nebula=true; otherwise pass native bytes.
5. Only fall through to read_native + Bash when the work is byte-level on
   non-source text (configs, fixtures, generated artifacts).

If check_nebula rejects your snippet, fix it and retry — the server returns
the same lex/parse error from write_code when it can't translate.

Built-in Read / Edit / Write / Grep on supported source extensions
(`.py .js .ts .go .swift .kt .rs .c`) are denied by this plugin's PreToolUse
hooks. Use read_code / read_native / patch_code / write_code instead.
Non-source files (md, yaml, json, toml, sh, …) pass through built-ins
as normal.

## Nebula minimal crib

    x := 42            // immutable (walrus)
    count = 0          // mutable
    def fib(n):        // long-kw + indent block
        if n <= 1: return n
        for i in 2..=n:
            ...
    fn add(a, b) = a+b // short-kw + expr body
    |x| x*2            // lambda
    xs.map { it*2 }    // trailing closure

Short (`fn rt fo lt mt`) and long (`def return for let mut`) keyword pairs
are both one `cl100k_base` token.
EOF
