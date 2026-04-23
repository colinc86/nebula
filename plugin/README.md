# Nebula — Claude Code plugin (v0.2)

A Claude Code plugin that hides source code behind the Nebula IL. The host
reads files through a compact canonical form (~16% fewer tokens than native
across 8 languages); writes and edits land on disk in the file's original
language, translated by the Nebula compiler's deterministic rule-based
pipeline. No Ollama dependency in the critical path.

## What it does

- **SessionStart hook** injects a Nebula primer + tool crib into the host's
  system context. One-shot, cached for the session.
- **PreToolUse hooks** deny built-in `Read` / `Edit` / `Write` on the 8
  supported source extensions, redirecting the host onto the MCP tools.
  Non-source files pass through built-ins as normal.
- **MCP server** (`nebula` binary in `bin/`) exposes eight tools:

  Read / search:
  - `read_code` — source file as Nebula IL (raw source ≤40 lines).
  - `read_native` — raw bytes, no IL conversion; escape hatch for non-source
    files or byte-exact patch construction.
  - `search_code` — grep; returns matches as Nebula snippets.

  Write / edit:
  - `write_code` — accepts Nebula code, translates to the file's native
    language on write.
  - `patch_code` — byte-level replace. Native by default; pass `nebula=true`
    to have `old_str` / `new_str` translated from Nebula before the match.
    Single-file or bulk-glob mode.
  - `translate_code` — standalone any-to-any translation across 8 languages.

  Validate (cheap, no file I/O):
  - `check_nebula` — does this snippet parse? Returns `{ok:true}` or
    `{ok:false,kind,line,col,message}`.
  - `preview_translation` — dry-run the Nebula → native translation so the
    host can inspect what `write_code` would produce before committing.

## Supported languages

Python, JavaScript, TypeScript, Go, Swift, Kotlin, Rust, C. Non-source files
(`.md`, `.yaml`, `.json`, `.toml`, `.sh`, …) pass through the built-ins
untouched.

## Installation

### Local dev

```bash
# From the nebula repo root:
cd compiler && cargo build --release && cd ..

# The plugin's binary symlink already points at compiler/target/release/nebula,
# so the release build is picked up automatically.

# Launch Claude Code with the plugin loaded (current session only):
claude --plugin-dir ./plugin
```

### Marketplace (future)

Once published: `claude plugin install nebula`.

## No runtime dependencies

v0.2 ships with rule-based translation only; no Ollama, no network I/O.

The standalone `translate_code` tool will automatically use an Ollama-hosted
neural translator if `NEBULA_OLLAMA_URL` points at one (default
`http://localhost:11434`, model `nebula-8lang-14b-q4` per `bench/finetune/MODELS.md`),
but nothing else in the plugin's critical path needs it. If Ollama is
unreachable, `translate_code` falls back to the same rule-based codegen the
rest of the plugin uses.

## Configuration

All env vars land in the plugin's `.mcp.json`. Defaults work out of the box.

| Variable | Default | Effect |
|---|---|---|
| `NEBULA_MCP_TOOLS` | preset to 8 tools | Limits the tool surface. Remove the env var to expose the `code_task` delegate tool too (used in the iter-20/23 benchmarks; not recommended for real-world use — see `bench/paper.md §8`). |
| `NEBULA_DISABLE_IL` | unset | `1` forces `read_code` to return raw source — useful for debugging. |
| `NEBULA_OLLAMA_URL` | `http://localhost:11434` | Ollama endpoint consulted by `translate_code` only. |
| `NEBULA_OLLAMA_MODEL` | `nebula-8lang-14b-q4` | Neural translator model for `translate_code`. |

## What this is not

- Not a replacement for reading native source. Bash `cat` / `grep` from the
  shell returns native bytes; this is intentional. The plugin intercepts at
  the tool boundary, not the filesystem.
- Not a fine-tuned host, but ships well with one. The v0.2 primer + validation
  tools (`check_nebula`, `preview_translation`) are tuned for a host that
  emits Nebula natively. A stock host (Sonnet, GPT-4.1 base) can still use the
  plugin — it'll lean on `read_code` for compressed reads and `patch_code`
  native mode for edits, skipping the Nebula-emit path.

## Research & honest scope

Phase 20 matched-pair benchmark (8 shapes, Sonnet host, N=3) shows a
**−51.2% weighted-mean host cost** for the full delegate path, but that's a
different mechanism than what this plugin ships — the `code_task` delegate
is *not* in the v0.2 tool surface (filtered out via `NEBULA_MCP_TOOLS`). The
real-world transfer story, for both the delegate and the plugin-in-isolation,
is in [`bench/paper.md §8`](../bench/paper.md#8-limitations): on arbitrary
multi-step tasks neither beats Sonnet-builtins today. The IL-compression side
(this plugin's main read-path lever) is validated independently: across 8
languages, Nebula averages 16.2% fewer tokens than source
([`bench/results.md`](../bench/results.md)).

Expect this plugin to help where the host would otherwise re-read the same
large file turn after turn. Expect no help on tiny one-Read-one-Edit tasks.
For a real production win, a Nebula-fluent host (fine-tuned frontier model)
is the missing piece — see [`bench/paper.md §8.1`](../bench/paper.md#81-future-work).
