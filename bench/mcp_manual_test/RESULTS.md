# Nebula MCP — Real-World Test Results

**Date:** 2026-04-12
**Host model:** Claude Sonnet 4.6 (via `claude -p`)
**Neural backend:** `nebula-8lang-7b:latest` on Ollama @ 192.168.68.94:11434
**Task:** Implement 12 stub Go functions across 3 files until `go test ./...` passes (template at `bench/mcp_manual_test/template/`).

## Round 2 update (after fixes)

After the round 1 results below, two structural fixes landed:

1. **Comment preservation through ingest** — `to_nebula_with_opts(.., keep_comments=true)` plumbs a flag through `minify` / `strip_comments_and_blanks` and the per-language ingesters. MCP's `read_code` and `search_code` opt in. The benchmark still runs the canonical (comments-stripped) form.
2. **`patch_code` MCP tool** — fifth tool that takes `old_str` / `new_str` in the file's **native** language (not Nebula) and does a byte-level replace. Sidesteps the round-trip fragility that makes `write_code` expensive and lossy.

Re-running the same task with both fixes:

| | Baseline | Nebula round 1 (avg of 2) | Nebula round 2 |
|---|---:|---:|---:|
| Tests passing | 12/12 | 12/12 | **12/12** |
| Turns | 13 | 19 | **15** |
| `read_code` calls | n/a | 6 (3 src + 3 tests) | **3 (src only)** |
| Write tool | Edit | write_code ×3 | **patch_code ×5** |
| Output tokens | 2,647 | 5,193 | **5,773** |
| Total tokens | 144,876 | 255,089 | **235,792** |
| vs baseline | — | +76% | **+63%** |
| Cost | $0.144 | $0.247 | **$0.216** |
| Wall clock | 33 s | 87 s | 87 s |

**The two fixes worked as designed but the headline didn't move enough to flip the verdict.** Round 2 is ~8% better than round 1 on total tokens and uses 4 fewer turns. The comment-preservation win is visible: the model only read the 3 source files this round (vs all 6 source+test files in round 1), because it could see the docstrings and didn't need the tests to figure out what `Chunk(xs, size)` should do. Patches replaced the full-file rewrites cleanly — no more spurious `import "strings"` build-break-and-sed dance.

What's still costing tokens:

1. **Cross-syntax verification.** The model read files in Nebula but `patch_code` requires native Go. Before the first patch, the model ran one `Bash: cat ...` over all 3 files (~2.1 KB of result) to verify the literal Go bytes for constructing accurate `old_str` strings. That's the cost of the read-Nebula / write-Go split, and the model's instinct here is reasonable given the design.
2. **The task is implement-heavy, not read-heavy.** Each function is ~10 lines of Go. The dominant token cost is the model thinking through and writing 12 implementations from scratch, not reading existing code. Compression helps reads; reads aren't where the budget goes.
3. **TitleCase needed a retry** (one extra patch + one tiny fix) because the first implementation collapsed whitespace. Not Nebula's fault — would have happened on any run.

**Verdict: still a token loss for this workload, but the gap closed and there are no more outright bugs.** For a fair test of where Nebula's compression actually pays off, the next test should be a **refactor** task on a larger codebase — rename a function across N files, or change the signature of a widely-used helper. There the read side dominates and the read-side savings (15% per file × dozens of files × no test-file rereads thanks to comments) should swamp the patch overhead.

### Round 2 trace summary
```
ToolSearch ×3 (deferred-tool resolution overhead, harness-specific)
read_code strings_util.go        →  saw docstrings, no need for tests
read_code slice_util.go
read_code map_util.go
Bash: cat *.go                    ←  the verification step (cost ~600 tokens)
patch_code strings_util.go (689→1245 chars)
patch_code slice_util.go  (677→1070)
patch_code map_util.go    (724→ 979)
Bash: go test ./...               ←  11/12, TitleCase failed
patch_code strings_util.go (436→ 419)   ←  TitleCase retry
patch_code strings_util.go ( 32→  16)   ←  small followup
Bash: go test ./...               ←  12/12 ✓
```

Round 2 changes are committed in:
- `compiler/src/ingest.rs` — `from_<lang>_with_opts` variants, `minify`/`strip_comments_and_blanks` take `keep_comments`
- `compiler/src/lang_detect.rs` — `to_nebula_with_opts` public entry
- `compiler/src/mcp.rs` — fifth tool `patch_code`, primer updated to advertise it and explain the native-language requirement
- All 261 cargo tests still pass.

## Round 8 — All seven fixes shipped, clean apples-to-apples ✅

After items 4-7 landed (skip canonicalize on `//` lines, tighter primer, Nebula-form `search_code`, token-aware `text_level_translate`), I reran both tasks side-by-side with TodoWrite enabled in both conditions and the same prompts. This is the most honest comparison so far — single trial, no cherry-picking.

| | BASELINE imp | NEBULA imp | BASELINE ref | NEBULA ref |
|---|---:|---:|---:|---:|
| Tests passing | ✅ | ✅ | ✅ | ✅ |
| Tool calls | 11 | 13 | 25 | **18** |
| Turns | 12 | 14 | 26 | **17** |
| Output tokens | 2,676 | 3,586 | 4,277 | **3,440** |
| Cache-read tokens | 122,113 | 174,173 | 189,254 | **130,065** |
| **Total tokens** | **145,579** | **190,440** | **212,537** | **147,043** |
| Δ tokens vs baseline | — | **+30.8%** | — | **−30.8%** |
| Cost (USD) | $0.155 | $0.154 | $0.192 | $0.174 |
| Δ cost vs baseline | — | **−0.7%** | — | **−9.5%** |
| Wall clock | 29 s | 46 s | 76 s | 61 s |

**Verdict (honest version):** Nebula MCP is **a clean win on refactor** (≈10% cheaper, ≈30% fewer tokens, fewer turns, faster wall clock) and **a draw on implement** (cost identical, more tokens but mix-shifted toward output which is priced higher). Both workloads are correct. Wall clock is mixed — refactor wins, implement is slower because each MCP call is ~3x slower than a built-in.

### Variance is real

The earlier rounds 3 and 4 were the lucky ends of the variance band:

| Round | Workload | Best Nebula result | What was special |
|---|---|---:|---|
| 3 | refactor | 130K tok / -62% | Model walked straight through, no TodoWrite, no retries |
| 4 | implement | 90K tok / -38% | Lucky first-pass impl of all 12 functions, no TitleCase retries |
| **8** | implement | **190K tok / +31%** | Typical run: TitleCase needed 2 retry patches |
| **8** | refactor | **147K tok / -31%** | Typical run: 9 reads → TodoWrite plan → 9 replace_all patches |

Single-trial swings of 2-3x in either direction are normal. Sonnet picks different strategies each run (use TodoWrite or not, retry buggy code or get it right first try, use sub-Agents or not), and each strategy has its own cost profile. To get reliable point estimates we'd need ≥3 trials per condition, which I haven't done.

The structural picture from across all rounds is consistent: **read-heavy workloads benefit from compression on cache_read, and those savings compound across turns**. Implement-heavy workloads where output dominates are more sensitive to model strategy variance and the specific shape of patches.

### What items 4-7 actually changed

Per-item smoke tests confirmed each fix works in isolation, but the round-8 comparison is the integrated picture:

- **Item 4 (skip canonicalize on `//` lines).** Doc-comment fidelity is now preserved through ingest — `// Reverse returns s with...` no longer becomes `// reverse returns s with...`. Verified by reading `cache.go` through `read_code`: the type-name casing in comments survives. Zero impact on benchmark numbers (the canonical mode still strips comments entirely).
- **Item 5 (tighter primer).** First attempt cut the primer from ~1.3 KB to ~670 B by dropping the tool-notes section, but the model immediately regressed in round 5 — it stopped trusting the preview and went back to Bash-cat-verifying files. **Restored** the patch_code workflow guidance and ended up at ~990 B; that version held in v8. The lesson: the primer's "how to use these tools" section is load-bearing, not waste.
- **Item 6 (Nebula-form `search_code`).** Now does a second pass over the canonicalized Nebula form of each file, so cross-language `find usages` works. The dedup heuristic suppresses Nebula-form matches within ±2 lines of any raw match. Visible in v8 nebula refactor: the model actually used this in v6 and got cleaner file discovery.
- **Item 7 (token-aware `text_level_translate`).** Replaced the substring-replace chain with a word-boundary-aware walker that also skips string literals. **Three new tests** (`text_translate_tests::rt_does_not_mangle_import`, `replacements_skip_string_literals`, `word_boundary_protects_partial_matches`) lock in the fix. `import` no longer becomes `imporeturn`. Total cargo test count: 264 (up from 261).
- **Bumped `SMALL_FILE_THRESHOLD` from 50 → 100.** Round 5 showed the model still cat-verified files in the 50-100 line range. Bumping the threshold pushes more files into the raw-passthrough path, eliminating the verification dilemma at the cost of skipping compression on more files. Worth it.

### Why round 5 went sideways and what fixed it

Round 5 (with the over-tight primer) regressed both tasks. The full chain:

1. Tighter primer dropped the explicit "don't cat to verify" instruction.
2. On the refactor task, the model started Bash-cat-ing files after `read_code` for medium-sized files.
3. cat output added ~600+ tokens per call to subsequent context.
4. Compounded across many turns of context re-shipping → 318K total vs 130K in round 3.

Round 6: restored the guidance (primer back to ~990 B) → cat behavior stopped → token usage dropped to 285K. Round 7 (TodoWrite disabled) was a separate experiment that backfired — without TodoWrite the refactor model decided to make 38 individual `patch_code` calls instead of 9 `replace_all`s, blowing up to 475K tokens. Lesson logged: TodoWrite is a planning tool the model uses to commit to a sensible strategy; disabling it removes the strategic anchor. Round 8 puts everything back to the right defaults.

### Build state after items 4-7

- `cargo test --release`: **264 passed**, 0 failed (3 new text-translate tests added)
- `cargo build --release`: clean apart from 8 pre-existing dead-code warnings unrelated to this work
- Files modified this round:
  - `compiler/src/ingest.rs` — added `canonicalize_keep_comments` wrapper, threaded through `convert_with_language` and per-language `_with_opts` functions
  - `compiler/src/lang_detect.rs` — rewrote `text_level_translate` with token-aware walker, added `replace_word_outside_strings` helper + 3 unit tests
  - `compiler/src/mcp.rs` — primer rewrite (twice — initial cut + restoration), `SMALL_FILE_THRESHOLD` 50→100, `tool_search_code` adds Nebula-form pass with line-proximity dedup

## Round 4 — Implement task with three fixes shipped ✅

After round 3 confirmed the structural argument, three of the strategy items landed in the MCP server:

1. **Small-file threshold** — files at or below 50 lines bypass Nebula ingestion entirely and are served as raw source. The MCP primer overhead exceeded compression savings on tiny files; this just stops paying it.
2. **Native preview header** — when a file is large enough to compress, `read_code` prepends the first 5 lines of raw source as a `//`-quoted block before the Nebula body, so the host model can anchor on the literal package/imports/style without needing a Bash `cat` to verify.
3. **Source cache** — per-process LRU keyed on `(path, mtime)` for `(raw, nebula)` pairs. Avoids re-ingesting on each tool call. `write_code` and `patch_code` invalidate the entry on write.

Re-running the round 2 implement task (12 stub Go functions across 3 files):

| | Baseline | Nebula r1 (avg of 2) | Nebula r2 (post-fixes 1) | **Nebula r4 (post-fixes 2)** |
|---|---:|---:|---:|---:|
| Tests passing | 12/12 | 12/12 | 12/12 | **12/12** |
| Tool calls | n/a | 17 | 14 | **8** |
| Turns | 13 | 19 | 15 | **9** |
| `read_code` calls | n/a | 6 | 3 | **3** |
| Bash verification (`cat`) | no | no | yes (~600 toks) | **no** |
| Output tokens | 2,647 | 5,193 | 5,773 | **2,772** |
| Cache-read tokens | 124,282 | 219,618 | 212,439 | **72,005** |
| Cache-create tokens | 17,936 | 27,303 | 17,555 | **15,514** |
| **Total tokens** | **144,876** | **255,089** | **235,792** | **90,304** |
| vs baseline | — | +76% | +63% | **−38%** |
| Cost (USD) | $0.144 | $0.247 | $0.216 | **$0.121** |
| vs baseline | — | +71% | +50% | **−16%** |
| Wall clock | 33 s | 87 s | 87 s | **31 s** |

**Verdict: ✅ Nebula MCP now beats vanilla Claude Code on this implement task.** 38% fewer total tokens, 16% cheaper, 31% fewer turns, slightly faster wall clock. This is the same workload that was a 76% loss in round 1.

### What each fix did

The three fixes compound in ways the unit test alone couldn't predict:

1. **Small-file threshold** — every file in the test repo is under 50 lines, so all 3 reads returned raw Go directly. The model never needed to translate Nebula syntax back to Go in its head, never wrote Nebula in `patch_code` calls, never doubted what was on disk. The "split-syntax" mental tax that ate round 2's budget just disappeared.
2. **No preview header was actually emitted** in this run because no files crossed the threshold. But that's fine — the threshold made the preview unnecessary here. The preview is the safety net for medium-sized files that DO get compressed.
3. **Source cache** — cache_read dropped 66% (212K → 72K) even though the model still made 3 reads. The cache effect is about what gets re-shipped through context on every subsequent turn: with caching, the MCP server doesn't re-ingest on each call, so the responses are tighter and reproducible across turns, which the prompt cache then de-duplicates more aggressively.

### Trace

```
ToolSearch                                           ← deferred-tool resolution
mcp:read_code strings_util.go                        ← raw Go (file <50 lines)
mcp:read_code slice_util.go                          ← raw Go
mcp:read_code map_util.go                            ← raw Go
mcp:patch_code strings_util.go (617→1141 chars)      ← single patch per file
mcp:patch_code slice_util.go  (574→ 967)
mcp:patch_code map_util.go    (672→ 927)
Bash: go test ./...                                  ← 12/12 ✓
```

8 tool calls total. No exploratory Glob, no `cat` verification, no retries — the model walked straight through.

### Five-round summary

| Round | Workload | Baseline | Nebula | Δ |
|---|---|---:|---:|---:|
| 1 | Implement 12 stubs (no fixes) | 144,876 | 255,089 | +76% |
| 2 | Implement 12 stubs (+ comments + patch_code) | 144,876 | 235,792 | +63% |
| 3 | Refactor across 9 files | 341,218 | 130,057 | **−62%** |
| **4** | **Implement 12 stubs (+ threshold + preview + cache)** | **144,876** | **90,304** | **−38%** |

Both workloads are now wins for Nebula MCP. Refactor by a wide margin (compression compounds across many turns of file context); implement by a smaller but still real margin (the small-file threshold + cache eliminated the friction that was eating round 2's budget).

## Round 3 — Refactor task on a 9-file Go module ✅

**Setup:** Synthetic but realistic ~12 KB Go module (`bench/mcp_manual_test/refactor_template/`) — a KV-cache project with 9 source files: `cache.go`, `cache_test.go`, `api.go`, `metrics.go`, `validator.go`, `logger.go`, `main.go`, `admin.go`, `snapshot.go`. The type `KVCache` and constructor `NewKVCache` are referenced 48 times across all 9 files.

**Task (identical in both conditions, no Nebula mention):**

> Rename the type `KVCache` to `SnapCache` and the constructor `NewKVCache` to `NewSnapCache` throughout this Go module. Every reference in every file must be updated. Use the file-editing tools — don't shell out to sed or perl. When you are done, run `go build ./...` and `go test ./...` to confirm the module still builds and all tests pass.

| | Baseline | Nebula MCP |
|---|---:|---:|
| `KVCache` references remaining | **0** | **0** |
| `go build ./...` | ✅ pass | ✅ pass |
| `go test ./...` | ✅ pass | ✅ pass |
| Tool calls | 28 | 23 |
| Turns | 29 | **22** |
| Output tokens | 3,673 | 4,031 |
| Cache-read tokens | 319,891 | **108,633** (-66%) |
| Cache-creation tokens | 17,565 | 17,377 |
| **Total tokens** | **341,218** | **130,057 (-62%)** |
| Cost (USD) | $0.217 | **$0.192 (-12%)** |
| Wall clock | 58.6 s | 78.1 s |

**Verdict: ✅ Nebula MCP wins decisively on token cost for read-heavy refactor work.** 62% reduction in total tokens, 12% reduction in cost, identical correctness. The headline number that matters — **cache-read tokens dropped 3x** (320K → 109K). This is the compounding effect of Nebula compression: each turn ships the file context through cache, so a 15% reduction per file becomes a 66% reduction across 9 files × ~22 turns of context.

### Why the cache_read collapse matters

Look at the per-axis breakdown:

- **Output tokens roughly tied** (3,673 vs 4,031). Both runs wrote similar-sized patches.
- **Cache_creation tied** (~17.5K each). Both runs paid roughly the same first-turn system-prompt cost.
- **Cache_read 3x smaller for Nebula** (108K vs 320K). This is the entire delta.

That's the structural argument validated: compression on the *read path* compounds across every subsequent turn that includes the file in context. On a 1-turn read it's a 15% saving; on a 20-turn refactor session it's a 66% saving on the dominant cost line.

### Trace highlights

**Baseline strategy** (28 tool calls, 29 turns): `find` → `Glob` → `Grep` to discover files, Read all 9 files, then `Edit` with replace_all per file (twice per file: once for `KVCache`, once for `NewKVCache`), plus 2 targeted Edits in `cache.go` for doc-comment blocks, final `Grep` to verify, `go build && go test`.

**Nebula strategy** (23 tool calls, 22 turns): spawned a sub-Agent for initial exploration (interesting choice — the agent results aren't compressed since it's a separate tool, but the model used one anyway), then `find` + `grep` via Bash, then `read_code` ×9, then **a single `patch_code` per file with `replace_all=true`** substituting `KVCache→SnapCache`. This catches `NewKVCache` as a substring, so 9 calls covered both renames. Final `go build && go test`. Cleaner strategy choice.

The Nebula run was 33% slower in wall clock (78s vs 59s) because each MCP tool call spawns the `nebula` subprocess and runs the tree-sitter ingest pass — ~3.4 s/call vs Claude Code's ~2.1 s/call for built-in tools. Source caching in the MCP server (item 5 in the strategy section) would close most of that gap.

### Putting all three rounds together

| Workload | Baseline tokens | Nebula tokens | Δ |
|---|---:|---:|---:|
| Implement 12 stub functions (round 1, before fixes) | 144,876 | 255,089 | **+76%** |
| Implement 12 stub functions (round 2, after fixes) | 144,876 | 235,792 | **+63%** |
| Refactor across 9 files (round 3) | 341,218 | 130,057 | **−62%** |

Same MCP, same model, opposite outcomes. The workload is the variable. **Nebula MCP is a tool to reach for on read-heavy / refactor / explore tasks, and a tool to leave alone for implement-from-scratch tasks.** That's the honest framing.

## Strategy: where Nebula MCP can actually win

The implementation-task results say something specific: **session-level token cost is dominated by output and thinking, not by reads.** The 15% per-file compression Nebula achieves on the static benchmark only matters when reads dominate the session — which they do for *some* workloads and not others.

**Workloads where Nebula structurally helps:**
- **Refactors.** Read N files, change one thing in each. The read side is the budget; patches are tiny.
- **Code review / "explain this".** Pure read, no writes. Compression is pure win.
- **Cross-language exploration.** A model comparing Python and Go versions of the same logic — Nebula normalizes both, so the comparison is cheaper.
- **Long-context tasks.** Loading 30+ files for a high-level question; 15% × 30 files becomes meaningful in absolute terms.
- **Boilerplate-heavy languages.** Java getters/setters, Go error checks, C# attributes — Nebula compresses ratios are higher when the source has more ceremony.

**Workloads where Nebula structurally hurts:**
- **Implementation tasks.** Writing new code from scratch. Output dominates; reads are tiny; the 15% on reads is wasted.
- **Small files.** The MCP primer (~325 tokens) is fixed overhead. On a 50-line file you save maybe 50 tokens of compression to pay 325 tokens of primer. Net loss until file size crosses a threshold.
- **Tasks needing target-language fluency.** The model thinks in Go/Python natively. Nebula adds a translation step in its head, which costs reasoning tokens even when the round-trip is cheap.

### Concrete improvements, ranked by effort × payoff

| | Change | Effort | Expected payoff |
|---|---|---|---|
| 1 | **File-size threshold for compression** in `read_code` — return raw source for files under N lines (~50). Skip the compression dance on tiny files where the primer overhead exceeds savings. | ~10 LOC | Removes net loss on small-file workloads. |
| 2 | **Native-source preview header in `read_code`** — prepend the first 5 lines of raw source as a comment block before the Nebula body. Eliminates the cat-to-verify step we saw in round 2 (the model was trying to anchor its syntactic mental model). | ~20 LOC | Saves the verification Bash call (~600 tokens per session). |
| 3 | **Tighter primer.** Drop the tool-notes section (descriptions cover it), shrink syntax examples. Target ~600 bytes from current ~1.3 KB. | ~5 min | Saves ~150 tokens of cache_creation per session. |
| 4 | **Skip canonicalize on comment lines** — currently `// Reverse returns` becomes `// reverse returns`. The model can recover but loses signal. | ~15 LOC | Better doc-comment fidelity through round-trip. |
| 5 | **Source caching in MCP server** (per-process LRU keyed on path+mtime). Avoids re-ingesting on every tool call within a session. | ~30 LOC | Lower MCP-side latency, especially for `patch_code` after a `read_code`. |
| 6 | **Nebula-form `search_code`** — also search the canonicalized Nebula text (currently only substring-matches the raw source, then translates the snippet for display). Cross-language `find usages` becomes possible. | ~20 LOC | Unlocks a workflow built-in `Grep` can't do. |
| 7 | **Improve `text_level_translate`** — the lenient codegen mangles `import` (the `rt`→`return` substitution overlap). Walk by token boundaries instead of substring replace. | 1-2 hours | Foundational fix; lets `write_code` be trusted again. |
| 8 | **Workload routing.** Document or auto-detect when to enable Nebula MCP. "Refactor / read-heavy → on. Implementation → off." Could be a CLAUDE.md heuristic in the user's project. | docs only | Sets expectations; avoids the implement-task trap. |
| 9 | **Fine-tune the host model on Nebula** (research). Endgame: a model that thinks in Nebula natively, removing the translation cost entirely. Out of scope for 1.0. | weeks | Resolves the structural read-Nebula / write-target tension. |

The first three are no-brainers and account for most of the gap on the implementation task. Items 4-6 are clean wins on read-heavy workloads. Items 7-9 are bigger investments.

### Follow-up plan (items 4-7, teed up but not implemented)

#### Item 4: Skip canonicalize on comment lines
**File:** `compiler/src/ingest.rs` — `canonicalize` function around line 1317.
**Change:** Wrap the body so lines starting with `//` (or `// `) are skipped from the snake_case / method-rename / type-normalize passes. They pass through verbatim.
**Why:** Right now `// Reverse returns s with...` becomes `// reverse returns s with...` because `canonicalize` snake_cases the entire text. The lossy result still works but signals less to the model and degrades doc-comment fidelity through the round trip.
**Risk:** Low. Comments aren't part of the canonical form for the benchmark; this is purely an MCP-mode quality fix. Add a `cargo test` case that ingests Go code with `// CamelCaseDoc` and verifies the comment survives.
**Effort:** ~15 LOC + 1 test.

#### Item 5: Tighter primer
**File:** `compiler/src/mcp.rs` — `NEBULA_PRIMER` const.
**Change:** Drop the "Tool notes" section entirely (the tool descriptions in `tools/list` already cover that material), shrink the syntax bullets to one line each, drop redundant examples. Target ≤600 bytes from current ~1.3 KB.
**Why:** The primer is loaded once into cache_creation per session — measured at ~325 tokens. A 50% reduction saves ~150 tokens per session. Compounds across many sessions.
**Risk:** Low, but watch for the model losing the "don't pre-translate" guidance and starting to write Go into `write_code`. Validate by re-running rounds 2 and 3 and confirming the trace shows the same tool-routing behavior.
**Effort:** ~5 minutes of editing + a re-run.

#### Item 6: Nebula-form `search_code`
**File:** `compiler/src/mcp.rs` — `tool_search_code`.
**Change:** Today `search_code` substring-matches the raw source then translates the matching snippet for display. Add a second pass that ALSO substring-matches the canonicalized Nebula form of each file. Merge results, dedup by (path, line), prefer Nebula matches when available (because they're cross-language-canonical).
**Why:** Built-in `Grep` only finds literal target-language matches. A Nebula search lets you query `def reverse` and find both Python `def reverse()` AND Go `func Reverse()` — they both canonicalize to `def reverse(...)` in Nebula form. This is something built-ins fundamentally can't do, and it's the most defensible "MCP unlocks new workflow" story.
**Risk:** Medium. Need to be careful about line numbering — the Nebula line that contains the match doesn't correspond directly to a source line. One option: emit both the Nebula line and the nearest source line range. Validate by querying for a function name across the existing `bench/corpus/` files (Python and Go versions) and checking we get hits for both.
**Effort:** ~30 LOC + at least one new test.

#### Item 7: Improve `text_level_translate` (round-trip robustness)
**File:** `compiler/src/lang_detect.rs` — `text_level_translate` function around line 110.
**Change:** Replace the substring-replace chain with a token-aware walker that:
1. Splits each line into identifiers / keywords / punctuation / strings.
2. Maps Nebula keywords to target-language keywords ONLY at token boundaries.
3. Maps Nebula type names (`Str`, `Int`, `Bool`, etc.) to target equivalents at token boundaries.
4. Leaves string literals and comments untouched.
5. Translates `def name(arg: Type) -> Ret:` to `func name(arg type) ret {` with proper brace balancing for Go specifically.
**Why:** Today's substring replace mangles `import` because the rule "rt → return" matches the `rt` inside `import`. A token-aware pass fixes the entire class of overlap bugs and makes `write_code` trustworthy enough to recommend over `patch_code` for full-file replaces.
**Risk:** Higher. This touches the fallback path for ALL `from_nebula_lenient` calls, which means ALL `write_code` and `translate_code` calls when neural is unavailable. Need to add round-trip tests for each language: ingest a real file → from_nebula_lenient → confirm the result still parses with a target-language tree-sitter.
**Effort:** 1-2 hours plus per-language round-trip tests.

### Items 8-9 (longer horizon, not in this batch)

- **Item 8:** Document workload routing in `bench/mcp_manual_test/RESULTS.md` (this file) and `README.md` so users know to enable Nebula MCP for refactor / explore work and skip it for greenfield implementation. Also consider a `--mode refactor` flag on the MCP that hard-disables the file-size threshold when the user knows they want compression on every file.
- **Item 9:** Train a small host-model adapter on Nebula-flavored editing (read Nebula, output Nebula `patch` instructions). This is the long arc that turns Nebula MCP from "an interesting compression layer" into "a model that thinks in Nebula natively" — out of scope for this debugging round but worth a research spike after items 1-7 land.

### What I'd validate next

The refactor test below is the immediate proof point. If Nebula wins (or at least breaks even) on a rename across 9 files, the structural argument is real and the open work is the fixes above. If Nebula still loses on a read-heavy task, that's a sign the primer overhead and tool-call friction are bigger than the compression itself, and the strategy needs to rethink whether per-tool-call compression can ever beat built-ins.

## Round 1 (initial findings — before the fixes)


## Headline

| Metric | Baseline (no MCP) | Nebula MCP run 1 | Nebula MCP run 2 |
|---|---:|---:|---:|
| Tests passing | **12 / 12** | **12 / 12** | **12 / 12** |
| Turns | 13 | 20 | 18 |
| Wall clock | 32.7 s | 73.8 s | 100.8 s |
| Output tokens | 2,647 | 4,450 | 5,936 |
| Cache-creation tokens | 17,936 | 37,570 | 17,042 |
| Cache-read tokens | 124,282 | 206,793 | 232,342 |
| **Total tokens** | **144,876** | **248,836** (+71.7%) | **261,342** (+80.4%) |
| Cost (USD) | $0.144 | $0.270 | $0.223 |

**Verdict: ❌ Correctness preserved, but the MCP layer is currently a token *loss*, not a save.** Two trials averaged ~76% more total tokens and ~94% more output tokens than the bare-Claude baseline.

## What worked

1. **Round-trip correctness.** All three runs produced compiling, test-passing Go. The 7B-model-assisted `write_code` path generated idiomatic Go (`for i := 0; i < len(xs); i += size`, `seen := make(map[int]bool)`, etc.) that's nearly indistinguishable from the baseline output.
2. **The primer landed.** The model's `write_code` calls contained genuine Nebula syntax — `fn Reverse(s: Str) -> Str:`, `i := 0`, indent blocks. The `instructions` field added to the MCP `initialize` response was sufficient for Sonnet to start producing Nebula without ever being told the word "Nebula" by the user. (Smoking-gun trace in `/tmp/nebula_stream.jsonl`.)
3. **No catastrophic confusion.** The model never refused to edit, never asked the user what the syntax was, never wrapped output in markdown apologies. It got on with the job.
4. **Tool routing worked.** With `--disallowed-tools "Read Edit Write Grep MultiEdit"`, the model went through `mcp__nebula__read_code` for all 6 file reads (3 source + 3 test) and `mcp__nebula__write_code` for all 3 writes — no attempt to escape to built-ins.

## Why it lost on tokens

Three concrete reasons, in priority order:

### 1. `write_code` is full-file-replace; built-in `Edit` is a diff
The biggest single cost. Each implemented function should be a ~10-line `Edit` against the existing stub. Instead, every `write_code` call had the model emit the **entire file** (~700-900 chars in Nebula form). Across 3 files that's ~2,400 chars of output the baseline never had to generate. Output tokens nearly doubled (2,647 → 5,193 avg).

This is the dominant effect and it's structural. Adding a `patch_code` MCP tool that takes a Nebula-form diff would close most of the gap.

### 2. Comments are stripped on `read_code`, so the spec can't live in the file
Nebula's ingester drops all comments during normalization. Smoking gun:

```
$ read_code strings_util.go
1  def reverse(s: Str) -> Str:
2    return ""
3  def title_case(s: Str) -> Str:
4    _ = unicode.is_space
5    return ""
6  def count_runes(s: Str) -> Int:
7    return 0
8  def is_palindrome(s: Str) -> Bool:
9    return False
```

All the docstrings explaining what `Chunk(xs, size)` should *do* are gone. For this test I worked around it by baking specs into the prompt for both conditions, but in the real use case (a user pointing Claude at a normal Go repo) the model would lose all docstring context on every read. That's a serious downside, not just a token issue.

### 3. The round-trip introduced a build break
Run 1 quote from the model:

> *"All 12 tests pass. The only fix needed was removing a spurious `\"strings\"` import the Nebula translator injected — none of the implementations actually use it."*

The 7B model (or the rule-based fallback) added an unused `import "strings"` that broke `go test`. Go is strict about unused imports, so the model had to compensate with a `sed` call. That's one extra round trip + one extra Bash call worth of tokens, plus a hit to user trust. `neural::validate_output` doesn't currently check for unused imports.

### Smaller contributors
- **Tool descriptions are bigger.** The 4 MCP tools have JSON-schema descriptions ~2-3x the size of compact built-ins. Adds to cache_creation cost on first turn.
- **Verification turns.** The Nebula model called `Bash: head -10 strings_util.go` after writing — uncertainty about what `write_code` actually produced. Built-in `Edit` doesn't trigger that because the model already knows what it sent.
- **`ToolSearch` overhead.** The harness deferred MCP tool schemas, so each first use of a new tool costs a `ToolSearch` round trip. ~3 occurrences in run 2. Real users on a vanilla Claude Code config wouldn't see this — it's a quirk of how I'm running it from inside an existing session.

## Confusion artifacts to flag

- **Hybrid Nebula/Go syntax in `write_code` input.** The model produced things like `fn Reverse(s: Str) -> Str:` (Nebula) followed by `runes := []rune(s)` and `unicode.IsSpace(r)` (raw Go). The lenient parser accepted it, but it's a sign the model isn't fully committing to Nebula — it's emitting "Nebula-flavored Go." Whether that's a problem depends on whether you want pure-Nebula edits.
- **One "I'll just write Go directly" moment** in run 2: the model said *"I'll implement all three files using direct Go writes for reliability with the type-specific operations."* — but then still went through `write_code`. So the moment of doubt didn't translate into actual built-in tool use, but it suggests the primer's "do NOT pre-translate" guidance isn't 100% sticky on every turn.

## What I changed during the test

| File | Change |
|---|---|
| `compiler/src/neural.rs` | `OLLAMA_URL` and `DEFAULT_MODEL` now read from env vars `NEBULA_OLLAMA_URL` / `NEBULA_OLLAMA_MODEL`. Added `"You are a code translator"` to `clean_output`'s hallucination-marker list (the 7B model echoes the system prompt at the end of generations). |
| `compiler/src/mcp.rs` | Added `NEBULA_PRIMER` const and returned it as the `instructions` field of the `initialize` response. ~1.3 KB primer. |
| `bench/mcp_manual_test/` (new) | `template/` (12-stub Go module), `prompt.txt`, `nebula_mcp.json`, `empty_mcp.json`, this `RESULTS.md`. |

All 261 tests still pass, build is clean (8 pre-existing dead-code warnings unrelated to these edits).

## Recommendations

In rough order of impact:

1. **Add a `patch_code` (or `edit_code`) MCP tool** that takes a unified-diff-style or `old_str`/`new_str` patch in Nebula form. This is the structural fix for the token loss. Without it, MCP edits will always cost more output tokens than built-in `Edit` for any task that doesn't rewrite full files.
2. **Preserve comments through ingest.** Even attaching them as opaque trailing tokens that survive the round trip would let the model see docstrings. Probably the right move is a `preserve_comments=true` flag on `lang_detect::to_nebula` defaulting on for MCP read calls and off for the canonical compression benchmark.
3. **Catch unused imports in `neural::validate_output`.** A simple check: if the output has `import ("strings")` (or single-line) but no `strings.` references in the body, drop the import. Same idea for Python. Cheap, prevents the build-break-and-sed dance.
4. **Audit MCP tool descriptions.** The current ones are concise but the schemas are still bigger than necessary. Consolidating `read_code` + `search_code` parameters could trim cache_creation overhead.
5. **Re-test with a refactor task** instead of an implementation task, so you can measure the read-side savings without the full-file-write penalty. A "rename function X across the package" task would isolate `read_code` and `search_code` from the `write_code` cost.

## How to reproduce

```bash
# 1. Build
cd compiler && cargo build --release

# 2. Stage the test
cp -R bench/mcp_manual_test/template /tmp/baseline_run
cp -R bench/mcp_manual_test/template /tmp/nebula_run

# 3. Baseline
cd /tmp/baseline_run && claude --print \
  --mcp-config /path/to/empty_mcp.json --strict-mcp-config \
  --tools "Bash Glob Read Edit Write Grep" \
  --permission-mode bypassPermissions --model sonnet \
  --output-format json --no-session-persistence --setting-sources user \
  "$(cat /path/to/prompt.txt)" > /tmp/baseline_result.json

# 4. Nebula
cd /tmp/nebula_run && claude --print \
  --mcp-config /path/to/nebula_mcp.json --strict-mcp-config \
  --disallowed-tools "Read Edit Write Grep MultiEdit NotebookEdit" \
  --permission-mode bypassPermissions --model sonnet \
  --output-format stream-json --verbose --no-session-persistence --setting-sources user \
  "$(cat /path/to/prompt.txt)" > /tmp/nebula_stream.jsonl

# 5. Score
cd /tmp/baseline_run && go test ./...   # expect: 12/12 pass
cd /tmp/nebula_run   && go test ./...   # expect: 12/12 pass
```

Trial count was 1 baseline + 2 Nebula (one to capture token totals, one to capture the stream-json trace). Variance between Nebula runs was substantial (24% on cost, 20% on cache_creation), so single-trial numbers should be treated as directional, not precise.
