# Phase 1 — Stock Baseline Report (G1) ✅ PASS

**Goal:** Confirm stock `qwen3-coder:30b` can drive the Nebula MCP agent loop, and establish the "before fine-tune" numbers.

**Date:** 2026-04-12
**Host model:** stock `qwen3-coder:30b` (Q4_K_M, 19.5 GB on the 3090)
**MCP server:** local `compiler/target/release/nebula mcp` with comments-preserved + `patch_code` + source cache + tighter primer (round-8 build)
**Translator backend (`write_code` neural):** `nebula-8lang-7b:latest` on the same host
**Runner:** `bench/finetune/host_agent_runner.py` (new this phase)

## Pass criteria (G1)

| Check | Target | Result | Pass? |
|---|---|---|---|
| Stock model drives the MCP loop without crashing | yes | yes | ✅ |
| ≥1 task completes with ≥50% test pass rate | yes | both tasks 12/12 | ✅ |

**Verdict: ✅ PASS — proceed to Phase 2.** Both the implement and refactor tasks finished correctly, on a fresh build, on a $0 local model. Strategy quality is mixed (see below) — exactly the gap we expect to close in fine-tuning.

## Headline numbers

| | Implement | Refactor |
|---|---:|---:|
| Tests passing | **12/12** ✅ | **12/12** ✅ |
| Turns | 13 | 47 |
| Tool calls (total) | 12 | 46 |
| Wall clock | 36.1 s | 66.4 s |
| Cumulative input tokens | 60,700 | 719,526 |
| Output tokens | 3,727 | 5,825 |
| API cost | $0 (local) | $0 (local) |

**Note on tokens:** Ollama does not implement prompt caching, so `input_tokens` here is the full re-evaluation of the conversation context on every turn — much larger than Sonnet's `cache_read` numbers. The right comparison is **wall clock** and **strategy efficiency** (tool-call count), not cumulative tokens.

## Implement task — clean run

12 tool calls in 13 turns. Reasonable strategy:

```
read_code strings_util.go
read_code slice_util.go
read_code map_util.go
patch_code strings_util.go     (full-file impl: old 868 → new 1657)
read_code strings_util.go      ← re-read (model wanted to verify)
patch_code strings_util.go     (refined: old 709 → new 1393)
patch_code slice_util.go       (full-file: old 697 → new 1172)
patch_code map_util.go         (full-file: old 744 → new 1156)
bash: go test ./...            ← failed on one case
patch_code strings_util.go     (small fix: old 52 → new 36)
bash: go test ./...            ← passed
bash: go test -v ./...         ← model double-checked verbosely
```

This is **basically the same shape as Sonnet round 8** for the implement task. Stock Qwen3-Coder is competent at implementation tasks. The only quirks: (1) one extra re-read of strings_util.go after the first patch (uncertainty), (2) double-checking with `go test -v` after the green run (not strictly necessary). Both small.

## Refactor task — verbose strategy, but correct

47 turns, 46 tool calls, 25 individual `patch_code` calls (NONE with `replace_all=true`). The model:

1. `search_code KVCache` → `search_code NewKVCache` (good — used the new feature!)
2. Read all 9 source files via `read_code`
3. **Patched each individual occurrence one at a time** (25 patches across the 9 files), some files re-read between patches
4. 8 `bash` calls for verification (`grep`, `cat`, `go build && go test`)
5. Final `bash echo` summarizing what it did (to satisfy the "stop with a final message" rule)

The Sonnet round-8 nebula refactor used **9 patch_code calls with `replace_all=true`** (one per file) for the same task. Stock Qwen3-Coder did **25 individual patches** instead. This is a **strategy gap, not a capability gap** — the model knows how to call `patch_code`, it just doesn't reach for `replace_all=true` on its own. This is exactly the failure mode we observed in v7 (Sonnet without TodoWrite), and exactly what fine-tuning on Sonnet traces should fix.

The verbose strategy is also why the cumulative input_tokens hit 720K — each turn re-evaluates the growing conversation context, so 47 turns of growing context dominates the budget.

## Comparison to Sonnet round 8

| | Sonnet+MCP (round 8) | Stock Qwen+MCP (this phase) |
|---|---:|---:|
| Implement turns | 14 | 13 |
| Implement tool calls | 13 | 12 |
| Implement wall clock | 46 s | 36 s |
| Implement test pass | 12/12 | 12/12 |
| Implement cost | $0.154 | $0 |
| Refactor turns | 17 | 47 |
| Refactor tool calls | 18 | 46 |
| Refactor wall clock | 61 s | 66 s |
| Refactor test pass | 12/12 | 12/12 |
| Refactor cost | $0.174 | $0 |

**Implement is at parity** (slightly faster wall clock for stock Qwen, which is interesting). **Refactor needs the strategy gap closed** — 2.5x more tool calls than Sonnet.

## What this confirms

1. **The MCP server and the runner work end-to-end** with a non-Anthropic host. The whole `claude -p` rig is no longer load-bearing.
2. **Stock Qwen3-Coder is capable enough** to drive multi-step agentic refactors on real code with Nebula MCP — no fine-tuning required to participate. Fine-tuning will move it from "capable but verbose" to "efficient and decisive."
3. **The right thing to teach in fine-tuning** is *strategy*, not capability:
   - Use `replace_all=true` for renames and other repetitive substitutions
   - Don't double-verify with extra `go test -v` calls
   - Don't re-read files immediately after patching them
   - Plan up front (like TodoWrite-style) instead of reacting per-result

Every one of these patterns shows up cleanly in Sonnet's traces. Distillation should transfer them.

## Bugs found and fixed during Phase 1

- **`host_agent_runner.py`** spawned the MCP server in the runner's cwd (the nebula repo root) instead of `task_dir`. This caused `search_code` to walk the whole nebula source tree and try to ingest C tree-sitter library files. **Fix:** pass `cwd=task_dir` to `subprocess.Popen` (`bench/finetune/host_agent_runner.py:60`). 1-line change.
- **`compiler/src/ingest.rs:935`** `postprocess_go` ternary handler had a slice-bounds bug: when `find(") ")` lands on a parenthesized expression that comes AFTER the `el` keyword (e.g. `} el if (foo->bar) {` from C code), `actual_close > el_pos` and the slice underflows. **Fix:** bail to `out.push(line)` if `actual_close >= el_pos` before computing the slice. 5-line guard. All 264 cargo tests still pass.

Both bugs were exposed by running the agent on a real codebase for the first time. The Phase 1 work paid for itself just by surfacing them.

## Files added/modified

**Created:**
- `bench/finetune/host_agent_runner.py` (~310 LOC, stdlib-only Python). Reusable for every subsequent phase that drives a local model through MCP. Supports both structured `tool_calls` and Qwen XML fallback parsing.
- `bench/finetune/qwen3_coder_30b/PHASE1_STOCK_BASELINE.md` (this file)

**Modified:**
- `compiler/src/ingest.rs` — slice-bounds guard in `postprocess_go`

**Test results captured at:**
- `/tmp/q1_implement.jsonl` (full per-turn log, 12 tool calls)
- `/tmp/q1_refactor.jsonl` (full per-turn log, 46 tool calls)

## Next phase

Phase 2: Build `bench/finetune/qwen3_coder_30b/generate_tasks.py` to produce ~200 task definitions across implement / refactor / bugfix / explain / cross-language. Reuse `bench/finetune/prepare_multi_target.py` extractors for source-file harvesting from StarCoderData / The Stack.
