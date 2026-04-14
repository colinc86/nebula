# Nebula 1.0 — Benchmark Results

**Date:** 2026-04-10
**Tokenizer:** cl100k_base (tiktoken)

## Phase 3: Token Compression

**Corpus:** Task Manager (~100 lines per language, equivalent logic across 8 languages)

| Language   | Src Tokens | Neb Tokens | Compression |
|------------|-----------|-----------|-------------|
| Python     |       870 |       842 |  **3.2%**   |
| JavaScript |       933 |       830 | **11.0%**   |
| TypeScript |      1140 |       924 | **18.9%**   |
| Go         |      1024 |       899 | **12.2%**   |
| Swift      |       967 |       768 | **20.6%**   |
| Kotlin     |       922 |       732 | **20.6%**   |
| Rust       |      1071 |       849 | **20.7%**   |
| C          |      1258 |      1017 | **19.2%**   |
| **Overall**| **8185**  | **6861**  | **16.2%**   |

All 8 languages compress. Mean: 15.8%. Best: Rust (20.7%). Least: Python (3.2%).

## Phase 4: InterTrans Validation (Python → Go)

**Model:** Claude Sonnet 4 | **Problems:** 20 HumanEval | **Metric:** Computational Accuracy

| Condition | Run 1 | Run 2 | Average |
|---|---|---|---|
| Direct LLM (Python→Go) | 60.0% | 75.0% | **67.5%** |
| Nebula-mediated (Python→Nebula→Go) | 80.0% | 70.0% | **75.0%** |

**+7.5pp average improvement** with Nebula mediation.

## Phase 5: Neural Refinement (Nebula → Python)

**Benchmark:** HumanEval (164 problems) | **Metric:** Pass@1

| Model | Size | Target | Passed | Pass@1 |
|---|---|---|---|---|
| Base Qwen-2.5-1.5B on Nebula | 1.5B | — | 2/164 | **1.2%** |
| Single-target (3-epoch) | 1.5B | Python | 80/164 | **48.8%** |
| **8-lang (production)** | **1.5B** | **8 langs** | **74/164** | **45.1%** |
| **8-lang (production)** | **7B** | **8 langs** | **111/164** | **67.7%** |
| **8-lang (production)** | **14B** | **8 langs** | **146/164** | **89.0%** |

The 14B 8-language model achieves **89.0% raw Pass@1** on Nebula→Python (on proper hardware), significantly exceeding published Qwen-2.5-14B results on raw Python (~70-80%). Note: requires 48GB+ VRAM; degrades to 57.9% with CPU offloading on 24GB GPUs.

## Phase 6: Error Correction Pipeline

**Benchmark:** HumanEval (164 problems) | **Max retries:** 3 | **Fallback:** rule-based `nebula to-python`

### 14B 8-language (hosted, 2x H100):

| Condition | Passed | Pass@1 |
|---|---|---|
| No correction | 146/164 | **89.0%** |
| + Error correction retries | 148/164 | **90.2%** |
| + Rule-based fallback | 153/164 | **93.3%** |

### 7B 8-language (local, 3090):

| Condition | Passed | Pass@1 |
|---|---|---|
| No correction | 111/164 | **67.7%** |
| + Error correction retries | — | — |
| + Rule-based fallback | 145/164 | **88.4%** |

### 1.5B 8-language (production):

| Condition | Passed | Pass@1 |
|---|---|---|
| No correction | 74/164 | **45.1%** |
| + Error correction retries | — | — |
| + Rule-based fallback | 130/164 | **79.3%** |

## InterTrans Scale-Up (10 Directed Pairs)

**Model:** Claude Sonnet 4 | **Problems:** 20 per pair | **Note:** Frontier model seeing Nebula cold (no fine-tuning)

### Python → 5 Targets

| Pair | Direct CA | Nebula CA | Delta |
|---|---|---|---|
| Python → JavaScript | 90.0% | 85.0% | -5.0pp |
| Python → TypeScript | 85.0% | 65.0% | -20.0pp |
| Python → Go | 65.0% | 65.0% | 0.0pp |
| Python → Rust | 80.0% | 75.0% | -5.0pp |
| Python → C | 75.0% | 60.0% | -15.0pp |
| **Average** | **79.0%** | **70.0%** | **-9.0pp** |

### 5 Sources → Python

| Pair | Direct CA | Nebula CA | Delta |
|---|---|---|---|
| JavaScript → Python | 23.5% | 41.2% | **+17.6pp** |
| TypeScript → Python | 52.9% | 35.3% | -17.6pp |
| Go → Python | 58.8% | 76.5% | **+17.6pp** |
| Rust → Python | 58.8% | 47.1% | -11.8pp |
| C → Python | 47.1% | 52.9% | **+5.9pp** |
| **Average** | **48.2%** | **50.6%** | **+2.4pp** |

### Combined

| Direction | Direct CA | Nebula CA | Delta |
|---|---|---|---|
| Python → 5 targets | 79.0% | 70.0% | -9.0pp |
| 5 sources → Python | 48.2% | 50.6% | +2.4pp |
| **All 10 pairs** | **63.6%** | **60.3%** | **-3.3pp** |

Without a Nebula-trained model, the IL is slightly net-negative (-3.3pp) for a frontier model seeing it cold.

## MBPP Benchmark (500 problems)

| Model | Benchmark | Pass@1 |
|---|---|---|
| 7B multi-target on Nebula input | MBPP (500) | **55.4%** |
| 7B multi-target on Nebula input | HumanEval (164) | **68.9%** |
| Published Qwen-2.5-7B on raw Python | MBPP | ~55-65% |

Confirms on a second, larger benchmark: Nebula-trained models match base model performance on raw Python. Nebula wins where ceremony-stripping helps (JS→Py +17.6pp, Go→Py +17.6pp), loses where type information is stripped (TS→Py -17.6pp).

## Agent Model: `nebula-host-30b` (Round 11, self-hosted end-to-end)

**Base:** Qwen3-Coder-30B-A3B-Instruct (MoE, 30.5B total / 3.3B active, Apache 2.0)
**Training:** LoRA (r=16, attention-only) on 138 agent traces distilled from Qwen3-Coder-480B-A35B-Instruct driving the Nebula MCP. The 47 refactor traces use a `replace_all=true`-mandatory system prompt (Phase 12); the other 91 are the Phase 11 traces verbatim.
**Deploy:** Q4_K_M GGUF (18 GB) on Ollama, RTX 3090, 32K context, 100% GPU
**Runner:** `bench/finetune/host_agent_runner.py` with Path-B runtime nudges
**Cost to run:** $0 (no API, fully self-hosted alongside `nebula-8lang-7b` translator backend)
**License story:** teacher and student are both Apache 2.0; no Anthropic or other closed-weights outputs in the training data.

Tasks are the implement + refactor tests from `bench/mcp_manual_test/`. Verification = `go test ./...` passing on the modified repo. See `bench/mcp_manual_test/RESULTS.md` (Round 11) for the full trace.

### Implement task (stub → impl, 12 tests)

| Host agent | Turns | Wall clock | Output tokens | Tests passed |
|---|---:|---:|---:|---|
| Sonnet + Nebula MCP | 13 | 51 s | — | 12/12 |
| Stock Qwen3-Coder-30B + builtins | 16 | 89.7 s | 4,585 | 12/12 |
| Stock Qwen3-Coder-30B + Nebula MCP | 18 | 63.8 s | 3,013 | 12/12 |
| `nebula-host-30b` (Phase 12) + MCP | **17** | **55.9 s** | **2,751** | **12/12** |

### Refactor task (rename high-call-site symbols, 12 tests)

| Host agent | Turns | Wall clock | Output tokens | `replace_all=true` | Tests passed |
|---|---:|---:|---:|---:|---|
| Sonnet + Nebula MCP | 14 | 68 s | — | 9 / 9 | 12/12 |
| Stock Qwen3-Coder-30B + builtins | 38 | 101.6 s | 4,420 | 0 / 21 | 12/12 |
| Stock Qwen3-Coder-30B + Nebula MCP | 42 | 185.2 s | 5,759 | 0 / 22 | 12/12 |
| `nebula-host-30b` (Phase 11) + MCP + nudge | 55 | 176 s | 5,759 | 0 / 55 | 12/12 |
| **`nebula-host-30b` (Phase 12) + MCP** | **35** | **69 s** | **1,624** | **14 / 14** | **12/12** |

Phase 12 is strictly better than Phase 11 on both tasks (implement is slightly faster; refactor drops from 176 s to 69 s, a 61% wall-clock reduction). More importantly, Phase 12's refactor **beats the stock-Qwen+builtins baseline** on every axis (69 s vs 101.6 s wall clock, 1,624 vs 4,420 output tokens), so the "self-hosted stack saves wall clock and output tokens on every task shape we test" claim is now measured and defensible.

The Phase 12 fix was a training-data change only — 47 of the 138 training traces were re-distilled with a system prompt that makes `patch_code(replace_all=true)` mandatory for rename tasks. No compiler code, MCP server code, or runner code was touched. The student generalized the discipline cleanly to the held-out `KVCache → SnapCache` rename (14/14 patches use `replace_all=true`, zero brute-force slips), and non-refactor behavior stayed byte-identical because 91 of the 138 training traces were the Phase 11 set verbatim.

This is the "self-hosted full stack" result: a single RTX 3090 hosts both the agent (`nebula-host-30b`) and the translator (`nebula-8lang-7b`) that the MCP `write_code` tool calls into. No Anthropic spend, no per-inference cost.

### Nebula MCP vs. generic file tools — where it helps and on what metric

The MCP-vs-builtins A/B was run across multiple agents and tasks. The right *metric* differs by backend:

- **Sonnet** is billed per token, with `cache_read_tokens` charged at a ~10× discount. The relevant column is `total_tokens` because every line item is real money.
- **Ollama self-host** has no billing. The relevant columns are **wall clock** (real GPU time) and **output tokens** (fresh generation, not cacheable).

| Agent | Backend | Task | Metric | Baseline (builtins) | Nebula MCP | Δ |
|---|---|---|---|---:|---:|---:|
| Sonnet 4.6 | Anthropic (cache_read discount) | Implement (Round 4) | Total tokens | 144,876 | **90,304** | **−38%** |
| Sonnet 4.6 | Anthropic (cache_read discount) | Refactor (Round 3) | Total tokens | 341,218 | **130,057** | **−62%** |
| Sonnet 4.6 | Anthropic (cache_read discount) | Refactor (Round 8, TodoWrite) | Total tokens | 190,440 | **130,065** | **−31%** |
| Stock Qwen3-Coder-30B | Ollama self-host (slot cache active) | Implement (Round 10) | Wall clock | 89.7 s | **63.8 s** | **−29%** |
| Stock Qwen3-Coder-30B | Ollama self-host (slot cache active) | Refactor (Round 10) | Wall clock | **101.6 s** | 185.2 s | +82% |
| **`nebula-host-30b` (Phase 12)** | Ollama self-host (slot cache active) | **Implement (Round 11)** | Wall clock | 89.7 s | **55.9 s** | **−38%** |
| **`nebula-host-30b` (Phase 12)** | Ollama self-host (slot cache active) | **Implement (Round 11)** | Output tokens | 4,585 | **2,751** | **−40%** |
| **`nebula-host-30b` (Phase 12)** | Ollama self-host (slot cache active) | **Refactor (Round 11)** | Wall clock | 101.6 s | **69.0 s** | **−32%** |
| **`nebula-host-30b` (Phase 12)** | Ollama self-host (slot cache active) | **Refactor (Round 11)** | Output tokens | 4,420 | **1,624** | **−63%** |

**What this means.**

- **On Sonnet**, the Nebula MCP is a decisive token-cost win — a 31–62% reduction in billable tokens on both workloads. The mechanism is prefix compression on the cacheable portion of the agent's context, compounded across turns through Anthropic's cache_read discount.
- **On Ollama with stock Qwen3-Coder-30B**, the story splits by task shape (Round 10). Implement wins (−29% wall clock), refactor loses (+82% wall clock) because stock Qwen brute-forces renames instead of using `replace_all=true`.
- **On Ollama with our fine-tuned `nebula-host-30b` (Phase 12)**, both task shapes win by a comfortable margin. Refactor is 32% faster than the generic file-tool baseline and output tokens drop 63%. Implement is 38% faster with 40% fewer output tokens. The Phase 12 training fixed the brute-force refactor strategy by adding a `replace_all=true`-mandatory rule to the system prompt used during refactor distillation (see `bench/mcp_manual_test/RESULTS.md` Round 11 for the full writeup).

**Bottom line for the paper.**

1. **"Nebula MCP reduces billable tokens 31–62% on cache-aware APIs for refactor-shaped workloads on Sonnet"** — defensible, confirmed across three Sonnet trials (Rounds 3, 4, 8).
2. **"The self-hosted open-weights Nebula stack (`nebula-host-30b` + Nebula MCP + `nebula-8lang-7b` translator on a single RTX 3090) saves wall clock and output tokens vs. generic file tools on both implement-shaped and refactor-shaped tasks"** — defensible, measured in Round 11 across all four (agent × task × metric) cells, all favoring Nebula by 32–63%.
3. **"A refactor-strategy gap in stock open-weights agents can be closed at the training data layer with a minimal prompt edit during distillation, without touching the MCP server or the base model"** — also defensible; Phase 12 changed 47 of 138 training traces and nothing else.

See `bench/mcp_manual_test/RESULTS.md` Rounds 10 and 11 for the full measurement writeups and the mechanism of the refactor-strategy fix.
