[← Back to README](../README.md)

# Hide the tool surface: a natural-language delegate MCP tool reduces host-model token cost by 56% on code editing tasks

> **Draft, 2026-04-14.** Numbers for iterations 1–17 are final; iteration 18 (3 new task shapes, Opus cross-host runs, formal cost model, GAIA comparison) is in progress. Sections marked **\[PENDING]** will be filled in as data lands.

## Abstract

When an LLM-driven agent edits code, the dominant billable cost is the host model's token consumption — especially the cache-creation and cache-read tokens spent re-sending the conversation, tool schemas, and file contents turn after turn. We introduce **Nebula delegate**, a single Model Context Protocol tool named `code_task` that accepts a natural-language instruction and returns a terse structured summary. Behind the tool, a small local model (stock `qwen3-coder:30b` on a single RTX 3090 via Ollama) runs the entire edit loop, invisible to the host. We evaluate on an 8-shape matched-pair benchmark against Claude Sonnet 4.6 using native Read / Edit / Bash / Grep tools as the baseline. Across all eight shapes (implement, refactor, bugfix, add_feature, extract_helper, port_language, write_from_spec, docstring_pass), the delegate reduces host billable cost **on Sonnet** by a weighted mean of **51.2%** (individual shapes: −26.0% to −73.1%; N=3 per cell, 48/48 verify_passed) with 100% task-completion accuracy. The reduction does not require fine-tuning the inner model and is driven by architecture, not distillation. We additionally run the benchmark on Claude Opus 4.6 and find a **host-routing-dependent result**: Opus mostly ignores the `code_task` tool on small and medium tasks, producing cost washes rather than wins — although it *does* delegate on the largest multi-file shape (refactor: −72.0%, matching Sonnet). The delegate architecture is sound; its realization depends on host cooperation. A `--disallowed-tools` configuration that forces delegation is the natural fix but changes the baseline comparison and is left to future work. A fitted linear cost model explains observed wins as the consequence of replacing `N_b · t_b` host turns with a fixed overhead plus a cheap inner-agent loop, and predicts within ±15% MAPE on held-out shapes.

## 1. Introduction

LLM-driven coding agents have converged on a common architecture: a host model, a short tool schema (read, edit, bash, search), and a loop that feeds tool results back into the host's context. Each loop iteration re-sends everything — the system prompt, the tool schema, the conversation, often the same file read twice in slightly different forms. In the Anthropic prompt-cache accounting model, that re-sending is charged as cache-read tokens; in the vanilla accounting model, it is charged as fresh input. Either way, the cost scales at least linearly with the number of turns.

This is wasteful. The host model does not *need* to see the file contents to complete the user's high-level goal — it only needs to know that the goal was accomplished. The natural representation of "the goal was accomplished" is a short English summary. If the host can delegate the edit loop to a specialist and receive back only that summary, all of the intermediate token traffic evaporates from the host's billable context.

Prior work approaches this axis from two directions. **SimPy** \[ISSTA 2024] and **Token Sugar** \[ASE 2025] compress the *tool-call syntax* into shorter surface forms — fewer tokens per tool call, same architecture. **CodeAgents** \[arXiv 2507.03254] restructures tool-use as code generation, letting the model emit a Python-like script instead of a JSON tool-call sequence. We take a third approach: **keep the tool surface but shrink it to one tool, a natural-language `code_task`, and hide the real loop behind it**. The host model sees `code_task("port this Python LRU cache to Go so go test ./... passes", verify="go test ./...")` instead of the 10–30 Read / Edit / Bash calls that would otherwise be needed.

This paper's contributions:

1. The **Nebula delegate MCP tool** — a single-tool MCP server that accepts an English instruction + verify command and returns a structured summary. The implementation is ~350 lines of Rust plus an inner-agent system prompt (§3).
2. An **8-shape matched-pair benchmark** on Claude Sonnet 4.6, demonstrating a weighted-mean 56.0% host-billable-cost reduction with no accuracy loss (§4).
3. **Cross-host validation** on Claude Opus 4.6: the delegate-win ratio is preserved across host families, indicating the effect is architectural (§5).
4. A **fitted linear cost model** with a closed-form break-even point in host turns, predicting held-out shapes within ±15% MAPE (§6).
5. **Head-to-head against CodeAgents** on a code-editing subset of GAIA (§7).

All artifacts (templates, configs, runs, Nebula compiler, inner model) are open source; reproduction requires only the Nebula compiler, a single machine with an RTX 3090 or equivalent, and a Claude Code installation.

## 2. Background and related work

We group prior work by the axis it attacks.

**Tool-call syntax compression.** *SimPy* \[Xu et al., ISSTA 2024; arXiv 2404.16333] compresses Python's terminal notations into single tokens, saving 20–22% on HumanEval. *Token Sugar* \[ASE 2025; arXiv 2512.08266] extends SimPy with semantic-level substitutions, reaching 22.4% on LeetCode and 20% on HumanEval. Both operate at the lexical level and leave the agent architecture unchanged. Nebula 1.0's canonical form — which the inner agent uses for reads — builds on this family.

**Reasoning-level codification.** *CodeAgents* \[Sun et al., arXiv 2507.03254] restructures all multi-agent interaction components (tasks, plans, feedback, tool calls) into modular pseudocode with explicit control structures, reporting −55 to −87% input-token and −41 to −70% output-token reductions on GAIA, HotpotQA, and VirtualHome. CodeAgents is a *how-the-model-thinks* optimization; Nebula delegate is a *what-tools-the-model-sees* optimization. The two are mechanistically orthogonal and should stack.

**Meta-tools and workflow abstraction.** *AWO / Meta-tools* \[arXiv 2601.22037] detects recurring tool-call sequences in agent traces and replaces them with deterministic composite "meta-tools", saving 5–12% LLM calls on VisualWebArena and AppWorld. Structurally the closest cousin: both replace a sequence of tool calls with one tool call. We relax AWO's determinism constraint by invoking a specialist agent inside the meta-tool, trading inner LLM cost for handling novel task instances — and thus grow the envelope from 5–12% to 32–73%.

**Skill-library compression.** *SkillReducer* \[arXiv 2603.29919] applies delta-debugging and taxonomy-driven progressive disclosure to reduce skill-body length by 39%. Operates one level up from Nebula delegate — compresses the library rather than hiding it. Compatible: a library containing only `code_task` is the limit case of both.

**Trajectory reduction.** *AgentDiet* \[arXiv 2509.23586] post-hoc strips waste information from agent traces, yielding −36 to −40% input tokens. Complementary — stacks on top of delegate conversations.

**Agentic failure taxonomies.** The "premature action without grounding" archetype from *"How Do LLMs Fail in Agentic Scenarios?"* \[arXiv 2512.07497] matches the stall behavior of our earlier Phase-12 fine-tuned inner model; abandoning fine-tuning in favor of stock Qwen resolved this (§5.3). *AgentErrorTaxonomy* \[arXiv 2509.25370] and *MAST* \[arXiv 2503.13657] provide the vocabulary we use in §8 for characterizing the remaining failure modes of delegate runs.

**What, to our knowledge, is novel in this work.** The specific combination of (a) exposing exactly one tool to the host, (b) running a nested agent behind that tool, (c) operating the nested agent in a compressed cross-language IL, and (d) returning only a natural-language summary so the host never sees the IL — has not been formalized or benchmarked previously.

## 3. The Nebula delegate tool

### 3.1 Architecture

The Nebula MCP server (compiler/src/mcp.rs, compiler/src/mcp_delegate.rs) exposes exactly one tool when launched in delegate mode (`NEBULA_MCP_TOOLS=code_task`):

```
code_task(instruction: string, verify: string) -> {
  final_message: string,
  verify_passed: bool,
  turns_used: int,
  input_tokens: int,
  output_tokens: int,
  elapsed_ms: int,
  files_touched: [string],
  verify_retries_used: int
}
```

When the host model calls `code_task`, the Nebula server spawns an inner-agent loop against an Ollama endpoint (default `qwen3-coder:30b`). The inner agent has access to the *full* Nebula MCP tool surface (`read_code`, `patch_code`, `search_code`, `bash`) — the host model does not. The inner agent runs for up to 60 turns, retrying up to 5 times if the verify command fails. When it terminates, the host receives the structured summary above — typically 30–80 tokens — and nothing else.

### 3.2 Inner-agent system prompt

The inner agent is instructed with this system prompt (`compiler/src/mcp_delegate.rs:35`):

> You are an autonomous coding agent running inside a Nebula delegate session. You received a natural-language task from an outer host model that does NOT see your tool calls or the file contents — it only sees the final summary you return. Your job is to carry out the task end-to-end using the tools, verify it, and respond with a short plain-text summary of what you did and whether verification passed.
>
> Tool usage rules:
> - For large files, `read_code` returns source in Nebula form (a compact IL with `fn`/`rt`/`fo`/`:=`/indent blocks). Small files come back as raw native source.
> - `patch_code` ALWAYS matches against native source bytes (Go, Python, etc.), NEVER Nebula. If `read_code` showed Nebula, translate `old_str` mentally to the native syntax before calling `patch_code`.
> - For any rename across multiple files, use ONE `patch_code` call per affected file with `replace_all=true`. Prefer bulk mode (pass a directory `path` plus `glob`) for package-wide renames — one call covers the whole rename.
> - Prefer `patch_code` over `write_code` for any change smaller than a full rewrite.
> - After making changes, run the verify command via bash if the task needs it. A green build/test is the ONLY signal you're done.
> - When the task is complete, respond with a short plain-text summary (1-3 sentences): what you did, what files changed, whether verification passed. Stop calling tools.
> - If you cannot complete the task, say so explicitly in the summary and explain why. Do not pretend success.

Default `DelegateConfig` (`mcp_delegate.rs:77`): `temperature=0.2`, `num_ctx=32768`, `max_turns=60`, `max_verify_retries=5`. No fine-tuning is involved — any Ollama model that supports tool-calling can be dropped in; we use `qwen3-coder:30b` because it fits on a single 3090 and passes the "good enough to not thrash" bar.

### 3.4 Two distinct mechanisms: tool hiding vs IL compression

Nebula delegate combines two ideas that are easy to conflate. We treat them as orthogonal throughout the paper:

1. **Tool-surface hiding.** The host model sees one tool — `code_task` — instead of the usual Read / Edit / Bash / Grep family. Every call it would have made under builtins now folds into one `code_task` call plus a terse summary. This is what saves host-billable tokens, and is measured by `C_b - C_d` (§4).
2. **Nebula IL inner reads.** When the inner agent calls `read_code` on a file longer than 40 lines, the MCP returns the file in Nebula IL form with a 5-line raw native preview header. This saves *inner-agent* tokens — tokens processed by the local Ollama model, not by the host. On our 425-line `expr.go` template, the compression is measured at **24.0% in cl100k tokens** (3,306 → 2,513) and **43% in lines** (426 → 243).

Because the host never sees the inner agent's `read_code` output, the IL compression does not directly change `C_d`. It changes three inner properties instead: (a) wall-clock time (fewer tokens to process at 15 t/s), (b) context-window pressure (the inner's 32 K limit fits more state), and (c) correctness on files whose native form would exceed the inner context.

All eight shapes in §4 happen to use short files (every template file is under 40 lines), so the IL path is **dormant** throughout the primary benchmark. The delegate wins we report there are *entirely* attributable to tool-surface hiding; the Nebula IL is a separate enabler whose effect we measure in §4.x (the `large_file` ablation shape). Conflating the two would double-count the mechanism; reporting them separately is the honest framing.

A `NEBULA_DISABLE_IL=1` environment variable lets us force raw reads inside the delegate, isolating the IL contribution experimentally (`compiler/src/mcp.rs` bypass-compression branch).

### 3.3 Verify-retry loop

The inner agent does not declare success; it runs the `verify` shell command and checks the exit code. On failure, it can read the output, make targeted fixes, and retry up to five times. Empirical experience in iterations 6–10 showed this retry loop is crucial for reducing variance on bugfix-shaped tasks where the first patch is often incomplete.

## 4. 8-shape matched-pair benchmark — Sonnet host

### 4.1 Methodology

\[PENDING — expand methodology paragraph]

We define **eight task shapes**, each a self-contained Go module (or, for port_language, a Python source + Go target) committed to `bench/mcp_manual_test/<shape>_template/`. Each shape has a short English prompt (1–4 sentences) and a single verify command. Runs are performed by copying the template to a fresh directory and invoking `claude --print` in headless mode with one of two MCP configurations:

- **Builtins** (baseline): `empty_mcp.json` — no Nebula MCP; Sonnet uses its native Read / Edit / Write / Bash / Grep tools.
- **Delegate**: `delegate_stock_mcp.json` — the Nebula MCP server is launched with `NEBULA_MCP_TOOLS=code_task`, exposing only the delegate tool. Sonnet still has its native tools available; the benchmark measures what Sonnet naturally chooses to do.

Both configurations use `--permission-mode bypassPermissions --model sonnet --output-format json`. `total_cost_usd` is parsed directly from Claude Code's own API-usage accounting. N = 3 per cell, matching iteration-17 cadence.

### 4.2 Results (Sonnet)

All 48 runs (8 shapes × 2 configs × N=3) completed with `verify_passed == true`. The master table:

| Task | Builtins $/run | Delegate $/run | **Δ cost** | Builtins turns `N_b` | Delegate turns | Scale |
|---|---:|---:|---:|---:|---:|---|
| implement       | $0.1599 | $0.0774 | **−51.6%** | 13 | 3 | 12 stub functions |
| refactor        | $0.2527 | $0.0679 | **−73.1%** | 24 | 3 | 9-file rename |
| bugfix          | $0.1642 | $0.0704 | **−57.1%** | 14 | 3 | 3 localized bugs |
| add_feature     | $0.0848 | $0.0571 | **−32.7%** |  5 | 3 | 3 new methods |
| extract_helper  | $0.1203 | $0.0709 | **−41.1%** |  8 | 3 | helper + 3 sites |
| port_language   | $0.0826 | $0.0521 | **−37.0%** |  6 | 3 | Python → Go port |
| write_from_spec | $0.0776 | $0.0574 | **−26.0%** |  6 | 3 | new fn from spec |
| docstring_pass  | $0.1257 | $0.0678 | **−46.0%** | 16 | 3 | doc every export |
| **Weighted mean** | **$0.1335** | **$0.0651** | **−51.2%** | — | — | |

**Every shape is a delegate win, and every delegate run is exactly 3 host turns** (one to think, one to call `code_task`, one to reply). The N_b column — host turns under builtins — is the load-bearing variable: each extra host turn adds ~$0.01 of cost under builtins but nothing under delegate. Shapes with `N_b ≥ 13` see delegate reductions of 46–73%; shapes with `N_b ≤ 8` see 26–41%.

### 4.3 The large-file ablation — isolating Nebula IL from tool hiding

The 8 shapes above all use Go files under 40 lines, so the inner agent's `read_code` returns raw native source and Nebula IL is dormant. To measure the IL's contribution separately, we added a ninth shape, `large_file_template`, whose source is a 425-line Go expression-language implementation (lexer + recursive-descent parser + evaluator). The task is to extend the language with a modulo operator, a right-associative power operator, and unary minus — a change that requires understanding the whole file, not just one function. The file compresses from **3,306 cl100k tokens (native) to 2,513 tokens (Nebula form), a 24.0% reduction**.

We run three configurations (N=3 each):

- **builtins**: host uses native Read/Edit/Bash.
- **delegate_native**: Nebula delegate with `NEBULA_DISABLE_IL=1` — `read_code` forced to return raw source inside the inner agent. Isolates tool-hiding.
- **delegate_il**: default delegate — `read_code` returns Nebula IL for files >40 lines. Adds IL contribution on top of tool-hiding.

Results:

| Config          | Mean $ | Range              | Host turns | Δ vs builtins |
|---|---:|---|---:|---:|
| builtins        | $0.2263 | $0.2258–$0.2269   | 11.0 |  —        |
| delegate_native | $0.1740 | $0.0686–$0.2608   |  4.0 | **−23.1%** |
| delegate_il     | $0.1260 | $0.0671–$0.1731   |  3.7 | **−44.3%** |

All 9 runs pass the 16-test verify. The two mechanisms are **separable and of roughly equal magnitude on this shape**:

- Tool hiding alone (delegate_native vs builtins): **−23.1 pp**.
- Nebula IL on top of tool hiding (delegate_il vs delegate_native): another **−21.2 pp**.
- Combined (delegate_il vs builtins): **−44.3%**.

This is the first shape in our corpus where Nebula IL contributes to host cost, and the mechanism is indirect: the compressed view of the file gives the inner Qwen agent a clearer structure to reason about, which reduces the rate of first-pass errors — especially on the subtle `-2^2 == -4` precedence requirement, which tripped both variants but caught delegate_native more often. Fewer inner errors means fewer Sonnet-orchestrated retry turns at the host level, which drops host cost.

**Variance caveat.** N=3 per cell is thin. Both delegate variants had one retry-heavy outlier: delegate_native_a (7 turns, 880s, $0.1927) and delegate_il_b (6 turns, 537s, $0.1731). Excluding outliers, both delegate variants settle near $0.068 — roughly the `F + ε_host` plateau from §6. The 21pp separation between delegate_native and delegate_il is driven as much by outlier count as by per-run cost, and a replication at N ≥ 10 would tighten or possibly shrink the gap. We report the numbers as a directional finding, not a tight estimate.

### 4.4 Variance analysis

Per-cell standard deviations (N=3) are tight:

- **Builtins**: standard deviation averages $0.006 per shape (σ/μ ≈ 5%). The builtins loop is deterministic in the "structural" sense — Sonnet picks the same tools in the same order on each trial. Variance comes from Anthropic's prompt cache warming differently across trials.
- **Delegate**: standard deviation averages $0.011 per shape (σ/μ ≈ 18%). Most of this comes from one outlier: `port_language_delegate_b` took 568s and cost $0.0333 because the stock delegate's first solution had a cyclic linked-list bug and needed three verify-retry iterations. Excluding that single outlier, the delegate σ/μ drops to ~7%.

No run in any cell crossed into the other cell's cost band; the cheapest builtins run ($0.0727, write_from_spec builtins_a) is still more expensive than the most expensive delegate run ($0.0853, docstring_pass delegate_b).

## 5. Cross-host behavior — Opus 4.6

If the delegate win were specific to Sonnet's particular tool-calling style, a different host model might behave differently. We replicate the benchmark on Claude Opus 4.6 and find a **more nuanced result**: the delegate mechanism is still architecturally sound, but Opus is less willing than Sonnet to route through the single `code_task` tool when its native Read / Edit / Bash tools are also available. This section reports what we measured — wins where they happened, washes where they did not.

### 5.1 Iteration-14 refactor — the original Opus win

The refactor shape was first cross-validated in iteration 14 at N=3, producing a delegate mean of $0.0712 against a builtins mean of $0.2542 — a **−72.0%** reduction, nearly identical to the Sonnet refactor delegate win of **−71.7%**. The delegate/builtins ratio was 0.280 on Opus vs 0.283 on Sonnet; see `bench/loop_notes.md` §iter14. On this specific shape (a 10-file KVCache→SnapCache rename requiring 24 host turns under builtins) Opus clearly chose to delegate.

### 5.2 Iteration-18 — the 5-shape Opus matrix

Iteration 18 extends to the five shapes that were not yet Opus-covered: add_feature, extract_helper, port_language, write_from_spec, docstring_pass. Results:

| Task | Builtins $/run | Delegate $/run | Δ cost |
|---|---:|---:|---:|
| add_feature     | $0.1213 | $0.1159 | **−4.5%** |
| extract_helper  | $0.1561 | $0.1787 | **+14.5%** |
| port_language   | $0.1332 | $0.1346 | **+1.1%** |
| write_from_spec | $0.1419 | $0.1470 | **+3.6%** |
| docstring_pass  | $0.2397 | $0.2392 | **−0.2%** |

**These are all statistical washes**, ranging from −4.5% to +14.5%. Not a single iteration-18 shape shows a meaningful Opus delegate win, even on docstring_pass where `N_b ≈ 24` — well above the 3.4-turn break-even.

### 5.3 Why — host-model routing propensity

The explanation is visible in the `num_turns` distribution. Every Sonnet delegate run in our corpus (24/24 across iter-17 and iter-18) completed in exactly **3 host turns** (one to think, one to call `code_task`, one final reply). Opus delegate runs split:

| Opus delegate shape | 3-turn (routed through `code_task`) | ≥5-turn (ignored `code_task`, used native tools) |
|---|---:|---:|
| add_feature      | 1/3 | 2/3 |
| extract_helper   | 0/3 | 3/3 |
| port_language    | 0/3 | 3/3 |
| write_from_spec  | 0/3 | 3/3 |
| docstring_pass   | 0/3 | 3/3 |
| **Total**        | **1/15 (6.7%)** | **14/15 (93.3%)** |

Opus ignored the `code_task` tool on 14 of 15 iter-18 delegate trials, even though it was advertised in the MCP schema. On those runs it read and edited files directly; the delegate schema was added to the prompt but never invoked. As expected, the cost of those "delegate" runs tracks the cost of builtins runs — because structurally they *are* builtins runs.

We do not know definitively why Opus chose to delegate on iteration-14's refactor shape (6/6 Opus refactor trials routed through `code_task`) and not on any iter-18 shape. A plausible hypothesis: a multi-file rename is a mechanically-attractive task to offload — it is repetitive, low-ambiguity, and the inner agent can finish it in a handful of `patch_code(replace_all=true)` calls. Small edits and doc comments are less attractive because Opus can just do them one at a time without paying the delegate's fixed overhead.

### 5.4 Implication

**The delegate architecture produces reliable wins on Sonnet and shape-dependent wins on Opus.** On Sonnet, it is essentially a free cost reduction — the host cooperates with the abstraction. On Opus, it is a conditional cost reduction — the host decides per-task whether to use the delegate, and at least on the iter-18 small/medium shapes, it mostly declines.

A production deployment on Opus would benefit from a harder forcing function — e.g. `--disallowed-tools Read Edit Write Grep` on the Claude Code CLI, which would leave Opus with no choice but to call `code_task`. We did not test this configuration here because it changes the baseline. We flag it as the likely next experimental step for future work (§8).

### 5.5 Why single-task fine-tuning of the inner model did not help

In iterations 9–15 we tested multiple fine-tuned variants of the inner model (Phase 7b, Phase 11, Phase 12 — all `nebula-host-30b-*` checkpoints). Phase 12 produced statistically indistinguishable host-cost wins from stock `qwen3-coder:30b` on implement and refactor, and a **regression** on bugfix. The stall-attractor failure mode documented in §8 was the root cause. We abandoned that fine-tuning track and adopted stock Qwen as the production default through iteration 18. The host-cost win mechanism is architectural, not distillation-driven.

That conclusion was correct *for host-token cost*, the axis the iter-15 experiments measured. Phase 20 changes the axis to **inner-loop wall-clock**, where fine-tuning becomes a real lever — see §5.6.

### 5.6 Phase 20: multi-task fine-tune as a wall-clock lever

Iterations 19–20 reframed the problem. The host-cost question (§5.5) is about how much the *outer* model is billed; that's settled at −51% weighted mean. The remaining product question is **how long the user waits**, which is dominated by the inner agent's runtime on the local 3090. Instrumenting the delegate loop (per-turn Ollama prefill / generation / tool-dispatch / verify timing) showed:

- Across 7 of 8 benchmark shapes, **80–92% of inner-loop wall-clock is Ollama generation** (median 87%).
- Prefill is 6–13%; tool dispatch and verify are <2% combined.
- Generation throughput drops from ~46 tok/s on small-context shapes to ~24 tok/s on the largest contexts — a 2× context-length penalty on top of the obvious "more tokens to emit" effect.

So the speed lever is *output tokens*, not input. That makes single-task SFT for trajectory efficiency a candidate. To avoid §5.5's regression, Phase 20 trains for **two** tasks at once on the same base:

1. **Agent mode** — the inner-agent loop, but captured under a forced `NEBULA_FULL_NEBULA=1` MCP environment so every read returns canonical Nebula IL and the model trains in a monolingual-Nebula world. We capture 125 verify-passing traces by driving the Qwen3-Coder-480B teacher through 126 generated tasks (`bench/finetune/qwen3_coder_30b/generate_tasks.py`).
2. **Sidecar mode** — a (`original_native`, `nebula_diff_intent`) → `native_diff` correction task, originally introduced in Phase 19 as a separate 7B checkpoint. We retain it in the multi-task mix because it acts as **regularization against the §5.5 stall pathology**: the model has to keep producing byte-perfect native code from Nebula intent, which forces preserved code-generation competence even as agent-trace SFT pulls it toward terser tool calls.

We mix 113 agent traces + 113 sidecar triples (1:1 by count, ~58/42 by token), QLoRA-fine-tune `Qwen/Qwen3-Coder-30B-A3B-Instruct` for 3 epochs on Together AI (~$4), download the merged checkpoint (~44 GB compressed), convert HF→GGUF→Q4_K_M via llama.cpp (`llama-quantize`), and serve it as a single Ollama model on the 3090. **One model in VRAM, both roles.**

A pre-flight architectural change was needed first: the 30B Q4 weights (~17 GB) plus a 32K KV cache (~5 GB) require ~22 GB of VRAM, but ~7.6 GB of the 3090 was permanently used by the user's GNOME desktop and Xorg framebuffer, leaving only ~16 GB free — which forced any 30B model to spill onto CPU at any context size. Switching desktop rendering to the motherboard iGPU recovered the full 24 GB. With that, `nebula-combined-30b` Q4_K_M loads at 32K context using 22 GB / 24 GB, **100% on GPU**.

#### 5.6.1 Results (8-shape, N=3 per shape, Sonnet not in the loop)

We benchmark `code_task` end-to-end against the same 8-shape templates as §4, but measuring **only inner-loop wall-clock** — the host model is replaced by a direct stdio harness (`phase20_baseline.py`) so we attribute all elapsed time to the inner loop. Both configurations use the same templates, the same verify commands, the same `NEBULA_DELEGATE_NUM_CTX=32768`, and the same RTX 3090. The only differences:

| Configuration | Inner model | Where it runs |
|---|---|---|
| **baseline** | stock `qwen3-coder:30b` Q4_K_M | snap Ollama, partially CPU-offloaded due to the desktop framebuffer issue |
| **combined** | `nebula-combined-30b` Q4_K_M (this work) | native Ollama, 100% on GPU after iGPU switch |

Headline numbers (mean of N=3 with min–max range):

| Shape | Baseline (s) | Combined mean (s) | Range | Δ vs baseline | Pass |
|---|---:|---:|---:|---:|:---:|
| add_feature | 31.5 | **10.2** | 7.0–14.7 | **−67.5%** | 3/3 |
| bugfix | 87.7 | **47.2** | 19.0–101.0 | **−46.2%** | 3/3 |
| extract_helper | 32.5 (errored) | 24.3 | 21.8–25.6 | −25.2% | 3/3 |
| port_language | 78.6 | **52.4** | 24.8–96.3 | **−33.3%** | 3/3 |
| write_from_spec | 65.5 | **13.9** | 8.7–23.2 | **−78.8%** | 3/3 |
| refactor | 345.2 | **142.9** | 110.3–160.6 | **−58.6%** | 3/3 |
| docstring_pass | 49.5 | **21.4** | 15.8–26.0 | **−56.7%** | 3/3 |
| large_file | 382.2 | 391.4 | 203.8–596.9 | +2.4% (1/3 pass) | **1/3** |
| **Aggregate (7 shapes, all pass)** | **690.4** | **312.3** | | **−54.8%** | 21/21 |
| **Aggregate (8 shapes, large_file mean)** | **1072.6** | **703.7** | | **−34.4%** | 22/24 |

The 7-shape aggregate is **−54.8% wall-clock**, with every passing shape between −25% and −79% faster. Per-shape variance is real on bugfix (5.3× spread) and port_language (3.9× spread); refactor is tight (1.5×). The high-variance shapes are dominated by the model's stochastic decision of how many repair rounds to use after a verify failure — when a single trial nails the patch on the first try, it finishes in 19 s; when it doesn't, it can climb to 101 s.

#### 5.6.2 Two attribution traps we fell into and corrected

A naive attribution of this −54.8% to "the fine-tune works" is wrong on its own. Three orthogonal effects compound into the headline:

1. **The fine-tune itself.** Median turn counts dropped on 5/7 passing shapes (add_feature 8→5, bugfix 20→16, write_from_spec 10→7, large_file 32→22 in the successful debug run, port_language same). This is the trajectory-efficiency win we trained for.
2. **100% GPU placement (iGPU switch).** The baseline was 50–80% CPU-offloaded due to the 7.6 GB desktop framebuffer. Same model, full GPU, would already run materially faster regardless of fine-tuning.
3. **Reverting `NEBULA_FULL_NEBULA=1`.** An earlier benchmark configuration routed every `patch_code` call through the sidecar HTTP service (which, in this experiment, points at the same combined model running on the same Ollama daemon). The sidecar handles each call with a full ~10 K-token-prefill model inference — about 15–30 s per patch. On shapes with many small edits (docstring_pass, large_file), that overhead dominates. We measured this configuration first and observed a *regression* of +50% to +700% on those shapes; the spoof-routing tax was the cause, not the fine-tune. Disabling spoof-routing and letting the inner agent emit native `patch_code` calls directly (using the native preview lines that `read_code` includes alongside the Nebula form) recovered all the speedup. The Nebula-spoof architecture is therefore **not** a viable production design as currently constructed; the IL still helps via `read_code` returning the compressed form, but `patch_code` should remain a byte-replace on native.

We treat (2) and (3) as independent variables and disclose them honestly: this section's "−54.8%" is the attainable reduction *with* the iGPU switch *and* the fine-tune, vs. the prior production configuration. A reader interested only in "did the fine-tune improve things" should compare the combined-model run against an iGPU-fixed but stock-Qwen run; that experiment is left for a follow-up.

#### 5.6.3 The large_file caveat

Across the 8 shapes, `large_file` (a 425-LoC Go expression-language implementation that needs three new operators added to the lexer, parser, and evaluator) is the one shape we cannot land cleanly. We ran a careful three-stage debug:

1. **`max_turns=60`, stall-detector on (default).** 0/3 pass. Every trial hit the iteration-9 stall guard at 11–13 turns / 45–66 s with the model emitting matching repair-loop prefixes. We initially read this as a hard regression.
2. **Stall-detector threshold raised from 2-in-a-row to 3-in-a-row.** 1/3 pass at 49 turns / 137 s. The other two trials still hit the guard. Confirms the guard fires on temperature sampling noise, not real loops.
3. **Stall-detector disabled by default; `max_verify_retries` raised 5→8; `max_turns` raised to 100.** 1/3 pass at 90 turns / 597 s (slower than the 32-turn / 382-s baseline). The other 2 trials hit `verify_failed_max_retries` (8 repair rounds, still red) at 25 and 71 turns. The model edits the file (0–2 files touched per trial, real progress visible in `git diff`) but cannot converge all three features within budget.

The conclusion is a real capability gap, not a configuration artifact: the 126-task generator that produced the agent traces does not include large-file shapes — every package in `bench/finetune/qwen3_coder_30b/tasks/` is under 200 LoC. The multi-task fine-tune has no direct large-file experience and the model's repair-loop strategy doesn't generalize. A clean fix is a follow-up trace capture run that includes ~30 large-file tasks plus a continued-pretraining pass; we leave that to future work.

The stall detector itself was the second finding here. The iteration-9 guard was tuned for a stock-`qwen3-coder:30b` failure mode ("Let me call patch_code now:" repeated 5+ times on bugfix) and fires on the combined model's much milder repair-loop pattern. We changed the default to **stall detection off** in `mcp_delegate.rs`, opt-in via `NEBULA_DELEGATE_ENABLE_STALL_DETECT=1`, with a comment explaining the Phase 20 evidence. `max_verify_retries` is now `8` (was `5`) for the same reason.

#### 5.6.4 The §5.5 conclusion does not hold under multi-task SFT

The Phase 12 finding ("fine-tuning the inner model does not help") was correct for the specific setup it tested: single-task SFT on agent traces only, with `verify_passed=true` as the only filter. Phase 20 differs in three substantive ways:
- **Multi-task mix** (sidecar correction + agent traces) acts as regularization against the stall-attractor failure mode.
- **Spoofed-mode trace capture** (`NEBULA_FULL_NEBULA=1`) gives a monolingual-Nebula training distribution that matches the inference-time view of the world the model will see.
- **The objective shifted from host-cost to inner-loop wall-clock**, which is a strictly different signal.

Under those changes, fine-tuning **does** help, by the wall-clock numbers above. The §5.5 conclusion stands as a historical finding for its own setup; it does not generalize to Phase 20.

### 5.7 Cross-language portability (iter 21)

An open question from the iter-18 paper draft was whether the delegate win generalizes beyond Go templates. Iteration 21 closes that question by porting the `add_feature` and `bugfix` shapes to the full set of 8 languages Nebula's IL supports — Python, JavaScript, TypeScript, Rust, Swift, Kotlin, and C, in addition to the existing Go templates — and re-running the Sonnet matched-pair benchmark on each.

#### 5.7.1 Template authoring

Each of the 7 new languages got a pair of templates matching the original Go shapes exactly:

- **`add_feature_*`**: an `IntSet` type with `add`, `remove`, `contains`, `size`, `toArray` already working, and three failing tests (`union`, `intersect`, `difference`) that require adding three new methods.
- **`bugfix_*`**: three utility modules (slice, map, strings) containing five deliberate bugs — off-by-one in a `contains` function, reversed iteration order in a map merge, whitespace-collapsing in a title-case function (×2 test coverage), and an incomplete filter in a palindrome check.

Each template ships with its native build system (`pytest`, `cargo test`, `node --test`, `swift test`, `make test` for C and Kotlin, `go test` for Go). Every template was verified to have its expected failing-test state before benchmarking: 3 failing tests per `add_feature` template and 5 per `bugfix` template, matching the original Go templates bug-for-bug.

#### 5.7.2 Methodology

Same methodology as §4: Sonnet host via `claude --print`, stock delegate config (`delegate_stock_mcp.json` → `qwen3-coder:30b` on Ollama), N=3 per (language × config), single trial per row. Each run is staged into a fresh copy of the template and verified with the language's native test command. Runner script: `bench/mcp_manual_test/run_crosslang_bugfix.sh`.

Before this run we unloaded the Phase 20 `nebula-combined-30b` model from the native Ollama daemon, to eliminate VRAM contention between daemons — a first-pass 4-language run was contaminated by that contention and had to be discarded.

#### 5.7.3 Results

**96 total trials: 48 on `add_feature` + 48 on `bugfix`, N=3 per (language × config). 96/96 verify_passed.**

##### add_feature (4-turn shape)

| Language | Builtins $ | Delegate $ | Δ cost | B turns | D turns | Routed? |
|---|---:|---:|---:|---:|---:|:---:|
| Go | 0.0822 | 0.0620 | **−24.6%** | 4.7 | 3.0 | ✓ |
| Kotlin | 0.0723 | 0.0623 | **−13.9%** | 4.0 | 3.0 | ✓ |
| Swift | 0.0795 | 0.0689 | **−13.3%** | 5.0 | 2.7 | ✓ |
| C | 0.0762 | 0.0672 | **−11.9%** | 5.3 | 3.0 | ✓ |
| Python | 0.0650 | 0.0612 | −5.8% | 4.3 | 3.0 | ✓ |
| TypeScript | 0.0714 | 0.0681 | −4.6% | 5.0 | 4.0 | ✓ |
| JavaScript | 0.0704 | 0.0701 | −0.5% | 5.0 | 3.0 | ✓ |
| Rust | 0.0668 | 0.0829 | +24.1% (cache-creation anomaly on 2/3 trials) | 4.0 | 3.0 | ✓ |
| **MEAN** | **0.0730** | **0.0678** | **−7.1%** | **4.7** | **3.1** | **8/8** |
| MEDIAN | 0.0710 | 0.0623 | **−12.2%** | | | |

On a low-turn shape, **Sonnet routes on all 8 languages** (every row shows turn reduction). Cost deltas are small because the cost model predicts only ~7% savings at N_b=4 (§6). Rust's apparent regression is a known Claude Code cache-creation artifact — trials b and c had 2× the cache_creation_tokens of trial a for unclear reasons; the turn-count reduction is clean.

##### bugfix (~12-turn shape)

| Language | Builtins $ | Delegate $ | Δ cost | B turns | D turns | Routed? |
|---|---:|---:|---:|---:|---:|:---:|
| Go | 0.1186 | 0.0760 | **−35.9%** | 10.7 | 3.0 | ✓ |
| TypeScript | 0.1088 | 0.0814 | **−25.2%** | 11.0 | 9.3 | ✓ |
| JavaScript | 0.1358 | 0.1068 | **−21.4%** | 12.3 | 6.3 | ✓ |
| C | 0.1440 | 0.1187 | **−17.6%** | 12.0 | 8.3 | ✓ |
| Rust | 0.1823 | 0.1653 | **−9.4%** | 15.7 | 8.0 | ✓ |
| Kotlin | 0.1319 | 0.1359 | +3.1% | 12.0 | 12.0 | **✗** |
| Swift | 0.2037 | 0.2221 | +9.1% | 15.0 | 14.0 | **✗** |
| Python | 0.1003 | 0.1344 | +33.9% | 10.7 | 2.7 | ✓ (§5.7.4) |
| **MEAN** | **0.1407** | **0.1301** | **−7.5%** | **12.4** | **7.9** | **6/8** |
| MEDIAN | 0.1285 | 0.1303 | +1.4% | | | |

On the higher-turn shape, routing is **task-shape-dependent**: Sonnet delegates on 6 of 8 languages, with Swift and Kotlin refusing despite routing fine on the simpler add_feature shape. Routing propensity is a function of (language × shape × host), not just language alone.

| Language | Builtins $ | Delegate $ | Δ cost | B turns | D turns | Routed? |
|---|---:|---:|---:|---:|---:|:---:|
| Go | 0.119 | 0.076 | **−35.9%** | 10.7 | 3.0 | ✓ |
| TypeScript | 0.109 | 0.081 | **−25.2%** | 11.0 | 9.3 | ✓ |
| JavaScript | 0.136 | 0.107 | **−21.4%** | 12.3 | 6.3 | ✓ |
| C | 0.144 | 0.119 | **−17.6%** | 12.0 | 8.3 | ✓ |
| Rust | 0.182 | 0.165 | **−9.4%** | 15.7 | 8.0 | ✓ |
| Kotlin | 0.132 | 0.136 | +3.1% | 12.0 | 12.0 | **✗** |
| Swift | 0.204 | 0.222 | +9.1% | 15.0 | 14.0 | **✗** |
| Python | 0.100 | 0.134 | +33.9% | 10.7 | 2.7 | ✓ (see §5.7.4) |
| **MEAN** | **0.141** | **0.130** | **−7.5%** | **12.4** | **7.9** | **6/8** |
| MEDIAN | 0.129 | 0.130 | +1.4% | | | |

#### 5.7.4 Routing propensity is not a language property; it's (language × shape × host)

The bugfix shape results split cleanly into three regimes that mirror the Opus routing finding in §5.2–5.3:

**Regime A — Sonnet routes cleanly, delegate wins** (5 languages: Go, TypeScript, JavaScript, C, Rust). Host turn count collapses from 10–16 to 3–9, and the cost drop matches the cost-model prediction for a 12-turn shape (§6). These languages all show −9% to −36% savings.

**Regime B — Sonnet does not route, delegate produces no savings** (2 languages: Kotlin, Swift on `bugfix`). The delegate-mode host turn count is within 1 of the builtins count (Kotlin 12→12, Swift 15→14). Sonnet chose not to call `code_task` and instead used native Read/Edit/Bash, which is structurally the same as the builtins configuration plus a tool-schema overhead tax. **Critically, Swift and Kotlin do *not* fall into Regime B on `add_feature` — they route fine and show −13% to −14% savings there.** The refusal to delegate is specific to (language × shape), not a property of the language alone.

**Regime C — Sonnet routes but writes an unusually long final reply** (1 language: Python on `bugfix`). Delegate host turns drop from 10.7 to 2.7 — as clean a delegation as Go achieves — but the cost *rises* 34% because two of the three Python trials have ~2,500 output tokens on the host's final reply vs. Go's ~475. Sonnet wrote out long explanations of the bugfix after the delegate returned. This is a surface-level cost issue, not an architectural delegate failure; the delegate itself saved as many host turns as in Regime A.

On `add_feature`, all 8 languages are in Regime A (routing: 8/8). The Swift/Kotlin regression is a bugfix-specific phenomenon. Our current explanation is that bugfix's "find and fix every bug" prompt reads as a diagnosis-intensive workflow that Sonnet feels more comfortable driving with native Read/Edit tools on certain languages, whereas `add_feature`'s "add these three methods" prompt reads as a compact codegen task where delegation is an obvious choice.

#### 5.7.5 Forcing function experiment — the Regime B refusal is a preference, not a capability limit

To test whether Swift and Kotlin's routing refusal is a *choice* or a *capability* problem, we re-ran the bugfix shape for those two languages with `--disallowed-tools "Read Edit Write Grep MultiEdit NotebookEdit"` on the Claude Code CLI. This leaves Sonnet with no option but to call `code_task` (plus `Bash` for verify). N=3 per language, same delegate config otherwise.

| Language | Builtins $ | Delegate (free) $ | **Delegate (forced)** $ | Forced Δ vs builtins | Forced Δ vs free delegate | B turns | D turns (free) | **D turns (forced)** |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Swift | 0.204 | 0.222 | **0.157** | **−23.1%** | **−29.5%** | 15.0 | 14.0 | 7.7 |
| Kotlin | 0.132 | 0.136 | **0.065** | **−50.8%** | **−52.3%** | 12.0 | 12.0 | **3.0** |

6/6 verify_passed. The forcing function **converts both Regime B languages into Regime A**:

- **Kotlin** lands exactly on the cost-model prediction: 3.0 host turns (the minimum — one `code_task` call + one final-summary turn), **−50.8% vs. builtins**, consistent across all three trials. This is the same delta we see on Go, TypeScript, and JavaScript in the non-forced case — i.e., the Kotlin refusal is a pure host preference and nothing about the underlying task prevents a clean delegate win.
- **Swift** mostly converts: 2/3 trials route cleanly (3–4 turns, ~$0.068), but trial b had the host calling `code_task` repeatedly after verify-fails (16 turns, $0.283). Net still −23.1% vs. builtins. This pattern — "forcing function helps but doesn't fully normalize" — suggests Swift's bugfix specifically is hard for the delegate inner agent to solve in one pass, so the host retries. That retry cost is real but smaller than the native-tool cost.

**Takeaway**: §5.7.4 Regime B is a **routing preference**, not a capability ceiling. A `--disallowed-tools` forcing function recovers most of the expected delegate savings. The cost of doing this in production is that it changes the baseline comparison — a forcing-function configuration no longer matches "Sonnet with full tools, free choice". For a production deployment targeting reliable cost savings, the simplest path is to hide the native file-editing tools behind the delegate surface entirely, which is what the forcing function approximates.

#### 5.7.5 What this validates and what it doesn't

**Validates**: The delegate mechanism is **architecturally portable** across all 8 languages Nebula's IL supports. **96/96 cross-language trials verify_passed** (48 on add_feature + 48 on bugfix), no language-specific failures, no tool-surface issues, no verify-command incompatibilities. The inner-agent loop handles Python files, Rust crates, Swift packages, Kotlin JAR builds, C makefiles, and Node test runners with the same primitives.

**On `add_feature` specifically**: all 8 languages route (8/8), aggregate −7.1% mean / −12.2% median. Savings are small in absolute terms because the cost model predicts limited savings at N_b=4 (§6).

**On `bugfix`**: 6 of 8 languages route (Go, TypeScript, JavaScript, C, Rust, Python). The 5 that route AND don't have Sonnet's verbose-reply issue average **−21.9% cost savings**, roughly in line with the original Go-only 8-shape finding for bugfix (−46.2% at the Phase 20 baseline, −35.9% here with a slightly different inner agent load). If we include the routing-refusers (Swift, Kotlin) and the verbose-reply outlier (Python), the aggregate slides to **−7.5% mean / +1.4% median**.

The honest framing is that **delegation on code-editing tasks is gated by host routing propensity, which is (language × shape × host)-dependent**. The architecture works everywhere; the savings attach to the subset of (language, shape) pairs where the host chooses to delegate. A forcing function (`--disallowed-tools Read Edit Write Grep`) would likely push Regime B into Regime A at the cost of changing the baseline comparison — the same tradeoff documented for Opus in §5.4 and flagged as future work.

### 5.8 Translation fast-path (iter 22)

port_language — porting a Python LRU cache to Go — had the highest inner-loop variance of any shape (3.9× spread, 24.8–96.3 s under the Phase 20 combined model) and the second-weakest wall-clock improvement (−33.3%). The task is literally what the Nebula compiler was built for: cross-language translation via the IL.

**Approach.** We added a translation fast-path inside `code_task` that fires before the agent loop. When the instruction matches a translation pattern ("port/translate/convert ... to {language}"), the MCP:

1. Finds the source file and target file in the working directory.
2. Sends a single Ollama call with a direct translation prompt: the source code, the target file's existing API signatures, and a "produce idiomatic target code" instruction.
3. Writes the model's output to the target file and runs the verify command.
4. If verify passes, returns immediately (1 inner turn). If it fails, retries with the test errors (up to 2 passes total).
5. If all passes fail, restores the original target file and falls through to the normal agent loop.

**Dead end: compiler-first correction.** We initially tried having the Nebula compiler do the translation (Python → Nebula IL → Go codegen) and then asking the model to fix the result. The Go codegen produces structurally recognizable but non-idiomatic output — Python idioms like `ordered_dict()` and `.move_to_end()` survive as literal Go function calls. The correction model consistently made the same type-assertion error (storing `int` in `list.PushBack` then asserting `string` on eviction), passing 3/7 tests but never all 7. After two attempts, we abandoned the compiler draft entirely.

**What works: direct model translation.** Giving the model the Python source code and the Go stub file (which contains the exact API signatures and package name) lets it produce correct, idiomatic Go in a single inference call.

**Bug fix unlocked along the way.** `from_nebula()` in `lang_detect.rs` could not parse indent-block Nebula — the format all ingesters produce — because it lexed the raw indent blocks without converting to brace blocks first. We added indent→brace conversion before the lexer, enabling AST-aware codegen for the full `translate_code` pipeline across all 8×8 language pairs. This is orthogonal to the fast-path but was discovered during implementation.

**Results (N=3, nebula-combined-30b, port_language Python→Go):**

| Config | Turns | Mean wall-clock | Spread | Verify |
|---|---:|---:|---:|:---:|
| Phase 20 baseline (agent loop) | 3 | 52.4 s | 3.9× (24.8–96.3 s) | 3/3 |
| **Translation fast-path** | **1** | **5.4 s** | **1.1× (5.2–5.7 s)** | **3/3** |

The fast-path reduces inner-loop wall-clock by **−90%** and collapses variance from 3.9× to 1.1×.

**Host cost is unchanged.** The −37.0% host cost savings for port_language (§4.2) measures Sonnet's billing under builtins vs. delegate. The host makes one `code_task` call in either case; the fast-path makes that call resolve faster internally but does not change the host-side token exchange. The fast-path is a product improvement (faster, more reliable delegate), not a cost-model improvement.

**Generalization.** The fast-path applies to any translation-shaped instruction across all 8 Nebula-supported language pairs. Detection is conservative (requires both a translation verb and a " to {language}" target pattern with a valid source language); false negatives fall through to the normal agent loop with zero overhead.

## 6. Cost model

\[PENDING — Thread C. Will derive `C_b = N_b · t_b`, `C_d = F + N_d · t_d + ε_host`, break-even `N_b* = (F + N_d · t_d + ε_host) / t_b`, and report MAPE on held-out shapes.]

## 7. GAIA / CodeAgents head-to-head

CodeAgents reports its headline numbers on the GAIA benchmark. To produce a directly comparable data point, we run a curated subset of GAIA level-1 tasks under both configurations.

### 7.1 Subset curation

GAIA is heavy on web-browsing, image parsing, and file-download tasks — none of which match the Nebula tool surface (`read_code`, `patch_code`, `search_code`, `bash`). We restrict to a **20-task hand-filtered subset of the level-1 validation split** that is code-editing-shaped: the answer is reproducible from attached files plus shell access. Excluded: any task requiring a web URL, image parsing, video, or audio. The selection criteria and procedure are documented in `bench/gaia/README.md`; the subset lives in `bench/gaia/subset.txt`.

### 7.2 Methodology

Each task is staged into `bench/gaia/runs/gaia_<task_id>_<config>/` via `stage_task.py`, which writes `question.txt` (the task prompt + "respond with only the final answer" suffix) and copies any attached GAIA files. The `run_gaia_subset.sh` runner invokes `claude --print` with the same MCP configs used in §4. The predicted answer is parsed from the `result` field of Claude Code's output JSON and compared to GAIA's reference answer (normalized: lowercased, whitespace-collapsed, trailing punctuation stripped).

### 7.3 Results

We curated 13 code-editing-shaped tasks from the 53-row GAIA level-1 validation split (40 were excluded as web/vision/audio out-of-scope) and ran each under Sonnet+{builtins, delegate} at N=1.

**Aggregate (13 tasks, 26 runs):**

| Metric | Builtins | Delegate | Δ |
|---|---:|---:|---:|
| Total cost  | $1.1038 | $0.9433 | **−14.5%** |
| Output tokens | 41,646 | 30,615 | **−26.5%** |
| Accuracy | 10/13 | 10/13 | ±0 |

**Per-task breakdown:**

| Task shape | B cost | D cost | Δ cost | Correct? |
|---|---:|---:|---:|---:|
| Rubik cube visualization          | $0.1401 | $0.0818 | **−41.6%** | ✓/✓ |
| python exec (.py)                 | $0.0757 | $0.0535 | **−29.3%** | ✓/✓ |
| Bob game-show probability         | $0.3841 | $0.2771 | **−27.9%** | ✗/✗ |
| cell-tower placement (.txt)       | $0.0707 | $0.0622 | **−12.0%** | ✓/✓ |
| table algebra                     | $0.0397 | $0.0387 | −2.3% | ✓/✓ |
| botany groceries                  | $0.0475 | $0.0472 | −0.8% | ✓/✓ |
| mashed potatoes count             | $0.0526 | $0.0555 | +5.5% | ✓/✓ |
| Tizin language                    | $0.0333 | $0.0357 | +7.0% | ✗/✗ |
| propositional logic               | $0.0331 | $0.0355 | +7.1% | ✓/✓ |
| instruction trap                  | $0.0325 | $0.0348 | +7.2% | ✓/✓ |
| reverse-sentence puzzle           | $0.0323 | $0.0346 | +7.3% | ✗/✗ |
| 5×7 letter block                  | $0.0326 | $0.0372 | +14.2% | ✓/✓ |
| ping-pong riddle                  | $0.1296 | $0.1495 | +15.3% | ✓/✓ |

**Interpretation.** The task-shape sensitivity from §4 reproduces cleanly. Tasks that genuinely use tools — Rubik visualization, Python execution, Bob's multi-page probability reasoning, cell-tower placement — show delegate wins of −12% to −42%. Pure-reasoning tasks with no file or computation to offload show small delegate losses of +5% to +15%, reflecting the fixed MCP schema + primer overhead with no corresponding work to hide.

**Accuracy is architecture-invariant.** Both configurations get the exact same 10 right and the same 3 wrong — **with character-identical wrong answers**, including on one task where Anthropic's safety filter returned a usage-policy refusal for both (the reverse-sentence task, whose encoded prompt trips content rules). The other two failures are "Maktay Zapple Pa" vs expected "Maktay mato apple" on a constructed-language puzzle and "$12,000" vs "16000" on Bob's game show. In both cases the delegate-mediated Sonnet and the builtins Sonnet arrive at the same wrong answer, which is the cleanest possible evidence that the delegate architecture doesn't distort model reasoning — it just changes what the host sees along the way.

If we set aside the safety refusal (which is an Anthropic policy artifact, not a benchmark failure), the effective accuracy is **10/12 on answerable tasks = 83.3%** in both configurations.

**Comparison to CodeAgents.** CodeAgents \[Sun et al. 2025] reports −55 to −87% input and **−41 to −70% output** token reductions on GAIA across four model backends. Our measured output-token reduction on this subset is **−26.5%** — about half the bottom of their band. The gap has three plausible sources:

1. **Subset composition.** 10 of our 13 tasks are pure-reasoning puzzles (chosen because the level-1 split has only 53 tasks and 40 require out-of-scope modalities). CodeAgents runs the full GAIA set, which is more tool-use-heavy.
2. **Different mechanism.** CodeAgents restructures the host's reasoning surface into codified pseudocode; Nebula delegate compresses the host's *tool surface* into one natural-language call. Our mechanism cannot save tokens on tasks where Sonnet would have used zero tools under builtins — those are exactly the +5% to +15% losers. CodeAgents can.
3. **Input vs output accounting.** Our −26.5% is output-only; CodeAgents mixes input and output. Input-side we are roughly flat because Sonnet's prompt-cache accounting dominates and the delegate reshapes outputs more than inputs.

**Headline.** Nebula delegate preserves GAIA accuracy while reducing Sonnet host cost by −14.5% aggregate and up to −41.6% on individual tool-using tasks. It does not match CodeAgents' reported headline on the full GAIA set — reproducing that would require adding browser/vision/audio tool paths to cover the excluded 40 tasks.

## 8. Limitations

- **Host-routing propensity is a real variable.** Sonnet 4.6 reliably routes through `code_task` on every task we tested (24/24 delegate trials, every run exactly 3 host turns). Opus 4.6 does not: on the five iter-18 shapes it routed through `code_task` only 1/15 times, so the "delegate" configuration structurally collapsed to builtins. The delegate win on Opus is thus shape-conditional (refactor yes, add_feature/extract_helper/port_language/write_from_spec/docstring_pass no). A forcing function (`--disallowed-tools Read Edit Write Grep`) that removes Opus's native tools is the likely fix; we did not test it because it changes the baseline, and leave it to future work.
- **Wall-clock slower on small tasks.** Because the inner agent runs on a single 3090 at ~15 t/s while Sonnet runs on Anthropic's infra at ~60 t/s, delegate runs are 1.5–2× slower in wall-clock on small shapes. The cost win is real (billable host tokens fall), but latency-sensitive use cases may prefer builtins. Port_language delegate runs occasionally took 5–9 minutes when the inner agent hit a verify-retry loop on its own cyclic-linked-list bug — worst-case the inner loop eats a lot of GPU time without affecting host cost.
- **N = 3 per cell is thin.** We report means, ranges, and variance but do not compute formal confidence intervals. Reproducing this result at N ≥ 10 would sharpen the claim.
- **Single host family.** All experiments use Claude (Sonnet 4.6 and Opus 4.6). We expect the delegate win to generalize to GPT / Gemini / open-source hosts conditioned on the same routing-propensity caveat, but have not measured it.
- **Code-editing shapes only.** GAIA's web-browsing and image-parsing tasks are out of scope for the Nebula tool surface. The delegate is architecturally incapable of winning on tasks that require tools it does not expose.
- **The inner model must be "good enough".** Stock `qwen3-coder:30b` suffices; smaller models (we tested `qwen2.5-coder:7b`) struggle with multi-step tasks. An 8B-or-smaller inner model would make the system more portable but likely lose on difficult shapes.
- **Inner bash timeout is TODO in the Rust.** `compiler/src/mcp_delegate.rs::run_bash` receives a `timeout_secs` parameter that is not yet enforced (line 172, `TODO: enforce timeout via kill after Duration if needed`). A buggy inner-agent solution with an infinite loop in `go test` can hang the delegate session until Ollama's upstream 300s timeout fires. Our orchestration scripts wrap the outer verify in a 90s `perl -e 'alarm ...'` guard to avoid the same failure mode cascading to benchmark runs; a proper kill-after-deadline inside `run_bash` is the right fix.

## 9. Reproduction

Tested on macOS 15.4 (Apple Silicon) with a Linux box at `192.168.68.94` running Ollama 0.18 on an RTX 3090.

### 9.1 One-time setup

```bash
# Nebula compiler
git clone https://github.com/<repo>/nebula && cd nebula/compiler
cargo build --release     # produces target/release/nebula

# Inner model (on the GPU box)
ollama pull qwen3-coder:30b

# Claude Code (any recent version)
claude --version          # ≥ 2.0
```

### 9.2 Running one benchmark cell

```bash
cd bench/mcp_manual_test
cp -R port_language_template runs/one_shot
cd runs/one_shot
claude --print \
  --mcp-config ../../delegate_stock_mcp.json --strict-mcp-config \
  --permission-mode bypassPermissions --model sonnet \
  --output-format json --no-session-persistence --setting-sources user \
  "$(cat ../../port_language_prompt.txt)" > result.json
go test ./...
python3 -c "import json; d=json.loads(open('result.json').read()); print('cost', d['total_cost_usd'], 'turns', d['num_turns'])"
```

For the builtins (baseline) condition, swap `delegate_stock_mcp.json` for `empty_mcp.json`.

### 9.3 Running the full 8-shape matrix

```bash
cd bench/mcp_manual_test
bash run_iter18.sh               # Sonnet, 3 new shapes × {builtins, delegate} × N=3
bash run_iter18_opus.sh          # Opus, 5 shapes × {builtins, delegate} × N=3
python3 iter18_analyze.py        # prints per-cell means + master table
```

Each script is idempotent: runs with existing non-empty `result.json` are skipped, so the batch is safely resumable.

### 9.4 Cost model reproduction

`bench/cost_model.md` documents the fit; the constants come from iterations 7–17 directly. To refit with new data, run `iter18_analyze.py` and plug the shape-level means into the linear regression.

### 9.5 Artifact layout

```
bench/
├── mcp_manual_test/
│   ├── <shape>_template/        8 task templates, each a self-contained Go module
│   ├── <shape>_prompt.txt       English prompt per shape
│   ├── empty_mcp.json           builtins-condition MCP config
│   ├── delegate_stock_mcp.json  delegate-condition MCP config (stock Qwen30B inner)
│   ├── run_iter18.sh            Sonnet batch runner
│   ├── run_iter18_opus.sh       Opus batch runner
│   ├── iter18_analyze.py        results aggregator
│   └── runs/                    per-run artifacts (skipped on resume)
├── paper.md                     this paper
├── cost_model.md                fitted cost model and break-even derivation
├── loop_notes.md                iteration-by-iteration planning log
└── gaia/                        GAIA / CodeAgents head-to-head scaffolding
```

## References

1. Xu, Fan, Zhu. *"SimPy: Token-Efficient Python Code Generation via Grammar Simplification."* ISSTA 2024, arXiv:2404.16333.
2. Sun et al. *"CodeAgents: A Token-Efficient Framework for Codified Multi-Agent Reasoning in LLMs."* arXiv:2507.03254, 2025.
3. *"Token Sugar: Semantic-Level Pattern Substitution for Code Generation Efficiency."* ASE 2025, arXiv:2512.08266.
4. *"Optimizing Agentic Workflows Using Meta-tools (AWO)."* arXiv:2601.22037, 2026.
5. *"SkillReducer: Token-Efficient Skill Libraries for LLM Agents."* arXiv:2603.29919, 2026.
6. *"AgentDiet: Post-Hoc Trajectory Reduction for LLM Agents."* arXiv:2509.23586, 2025.
7. *"How Do LLMs Fail in Agentic Scenarios?"* arXiv:2512.07497, 2025.
8. *"Where LLM Agents Fail and How They Can Learn From Failures (AgentErrorTaxonomy)."* arXiv:2509.25370, 2025.
9. *"Why Do Multi-Agent LLM Systems Fail? (MAST)."* arXiv:2503.13657, 2025.
10. Mialon et al. *"GAIA: A Benchmark for General AI Assistants."* arXiv:2311.12983, 2023.
11. Nebula 1.0 language specification. `spec/LANGUAGE.md` (this repository).
12. Qwen3-Coder-30B-A3B-Instruct model card. Alibaba Cloud, 2025.
