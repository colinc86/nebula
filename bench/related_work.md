# Related Work — Delegate-Coder Architecture and Agent Token Efficiency

Compiled 2026-04-14 during iteration 13 of the /loop. Focused on positioning
the Nebula delegate MCP tool against recent arXiv work on LLM agent token
efficiency, tool use, failure modes, and intermediate representation hiding.

## Papers most relevant to this work

### 1. CodeAgents (2507.03254) — closest magnitude comparison

**Sun et al., "CodeAgents: A Token-Efficient Framework for Codified Multi-Agent Reasoning in LLMs" (2025)**

- **Mechanism:** Codifies all multi-agent interaction components (tasks, plans, feedback, system roles, tool calls) into modular pseudocode with explicit control structures.
- **Results:** −55-87% input tokens, −41-70% output tokens across GAIA, HotpotQA, VirtualHome. SOTA 56% on VirtualHome.
- **How it compares to Nebula delegate:** Same magnitude of savings (Nebula: −53% cost, −70% output; CodeAgents: −55-87% input, −41-70% output). **Different mechanism entirely** — they compress multi-agent reasoning into pseudocode, we hide a compressed-IL-native sub-agent behind one tool call. CodeAgents is a "how the host model thinks" optimization; Nebula delegate is a "what tools the host model sees" optimization. The two are complementary, not competing.
- **Citation framing:** This is the paper to cite as the most direct magnitude comparison. "Our matched-pair benchmark shows host-cost reductions of 45-72% on implement and refactor tasks — in the same range CodeAgents reports for codified multi-agent reasoning, but achieved through a different mechanism (delegate tool + specialist inner agent) that preserves the host model's native tool-use vocabulary."

### 2. Meta-Tools / AWO (2601.22037) — structural cousin

**"Optimizing Agentic Workflows using Meta-tools" (2026)**

- **Mechanism:** Automatically detects recurring tool-call sequences in agent execution traces and replaces them with deterministic composite "meta-tools" that bypass intermediate LLM reasoning.
- **Three-stage pipeline:** State graph construction → horizontal merging (semantically equivalent states) → vertical merging (high-frequency path extraction).
- **Results:** −5.6-10.2% LLM calls on VisualWebArena, −7.2-11.9% on AppWorld; up to +4.2pp task success improvement.
- **How it compares to Nebula delegate:** Structurally identical idea at the 10,000-foot level — both replace a sequence of tool calls with one tool call. **Key differences:**
  1. Meta-tools are deterministic; Nebula delegate invokes a nested LLM agent for the sub-task. Meta-tools are more conservative (no runtime LLM overhead inside the meta-tool) but only work for sequences discoverable in training traces. Nebula delegate handles novel tasks.
  2. Meta-tools save 5-12% LLM calls; Nebula delegate saves 45-72% host cost. Much larger envelope because the delegate isn't bound to deterministic substitution.
  3. Meta-tools don't compress the underlying file representation; Nebula delegate runs reads in a compressed IL inside the inner agent.
- **Citation framing:** "Our `code_task` tool is philosophically aligned with AWO's meta-tools — both collapse multi-step tool-call sequences into single callable operations. We relax AWO's determinism constraint by invoking a specialist agent inside the meta-tool, trading runtime LLM cost for handling novel, non-pre-observed task instances. The per-call overhead of a 30B-A3B MoE on a single GPU is ~2-5s × ~5-20 turns, but the host-cost savings (45-72%) far exceed AWO's 5-12% LLM-call reduction because the host model never pays for file-level reasoning."

### 3. SkillReducer (2603.29919) — complementary skill-library optimization

- **Mechanism:** Studies 55K+ LLM agent skills and finds systemic token waste. Applies delta-debugging-based routing compression (48%) and taxonomy-driven progressive disclosure (39% body reduction).
- **How it compares:** Operates one level up from Nebula delegate — compresses the skill library the host model sees, rather than encapsulating skills into a single tool. Compatible with delegate approach (a skill library containing just `code_task` would be the limit case of both).

### 4. AgentDiet / Trajectory Reduction (2509.23586)

- **Mechanism:** Post-hoc trajectory reduction that removes waste information from agent execution traces without hurting performance.
- **Results:** −35.9-39.9% input tokens.
- **How it compares:** Post-hoc compression of agent trajectories vs Nebula delegate's pre-hoc abstraction via the tool surface. AgentDiet is a context-window optimization; Nebula delegate is a tool-surface optimization. Could stack (apply AgentDiet to the shortened delegate-mode conversation history).

### 5. SimPy (2404.16333) and Token Sugar (2512.08266) — lexical-level grammar compression

Already documented in `bench/experiments.md` as the primary precursors. Key framing for the paper:

- **SimPy** is Python-specific; compresses Python's terminal notations into single tokens (20-22% on HumanEval).
- **Token Sugar** extends SimPy with semantic-level pattern substitution (22.4% on LeetCode, 20% on HumanEval).
- **Nebula compression** is multi-language (8 ingesters) and achieves 16.2% cross-language compression on the Task Manager corpus — smaller single-language improvement but much broader applicability.
- **Nebula delegate** builds ON Nebula compression by operating the inner sub-agent in the compressed form while presenting a normal NL interface to the host.

## Papers relevant to the bugfix failure mode

### 6. "How Do LLMs Fail In Agentic Scenarios?" (2512.07497)

- **Finding:** Llama 4 Maverick and similar models often begin tasks with correct reasoning but "performance frequently degrades mid-execution, including failures due to malformed tool calls and loss of structure in JSON output".
- **Four failure archetypes:**
  1. Premature action without grounding
  2. Over-helpfulness that substitutes missing entities
  3. Vulnerability to distractor-induced context pollution
  4. Fragile execution under load
- **Direct match to Nebula's bugfix stall:** `nebula-host-30b`'s "Let me call patch_code. Let me do that now:" pattern is archetype (1) — premature action without grounding. The model emits a prefix announcing an action but never executes it, because at temperature 0.2 the output distribution terminates the turn before the tool call.
- **Paper framing:** "The stall-repetition pattern we observe in our specialist inner model (Phase 12 nebula-host-30b) matches the 'premature action without grounding' archetype documented in (2512.07497). We were unable to fully mitigate this via external tooling (6 distinct attempts across 4 iterations); the fix requires training-data intervention, motivating our planned Phase 13 re-distill."

### 7. "Where LLM Agents Fail and How They can Learn From Failures" (2509.25370)

- **Finding:** "LLM agents show vulnerability to cascading failures, where a single root-cause error propagates through subsequent decisions."
- **Contribution:** AgentErrorTaxonomy (memory / reflection / planning / action / system-level) and AgentDebug framework.
- **How it relates:** Our iteration-10 silent non-completion on refactor (`refactor_iter10_e` — Sonnet reported success when the rename was never done) is exactly the cascading-failure pattern. Root cause was a too-lenient verify command; the inner agent's failed patch_code propagated through the bash go-test call (which passed on unmodified valid Go) into a `verify_passed: true` signal that Sonnet then reported upstream. We detect this via `ZERO_FILES_MODIFIED` + `STALL_PREFIX_IN_FINAL` warnings in the delegate result — a lightweight version of AgentDebug's root-cause framework.

### 8. "Why Do Multi-Agent LLM Systems Fail?" (2503.13657) — MAST-Data taxonomy

- **Contribution:** 1600+ annotated traces across 7 MAS frameworks; 14-mode failure taxonomy in 3 categories (system design, inter-agent misalignment, task verification).
- **How it relates:** Nebula's `code_task` is a 2-agent system (host + inner delegate). The bugfix stall pattern falls under "task verification" (the inner agent claims done but didn't do it) and "inter-agent misalignment" (Sonnet's expectations of code_task's reliability don't match Phase 12's actual behavior on iteration-heavy shapes). Iteration 11's warnings (`ZERO_FILES_MODIFIED` / `STALL_PREFIX_IN_FINAL`) are task-verification signals that surface delegate failures to the host.

## What's genuinely novel in Nebula's approach

After the review, the novel contributions are:

1. **Compressed-IL inner agent behind a single MCP tool.** The closest neighbor (AWO meta-tools) keeps the underlying representation the same; CodeAgents restructures reasoning but doesn't compress; SkillReducer compresses skill descriptions but doesn't hide them. Nebula delegate is the first architecture I've seen that (a) exposes ONE tool to the host, (b) runs a nested agent behind it, (c) operates that nested agent in a compressed cross-language IL, and (d) returns the result as a natural-language summary so the host never sees the IL at all. This is the "free reign" direction the user's loop prompt asked for.

2. **Matched-pair 3-shape benchmark methodology with per-shape correctness verification.** Iteration 10's silent non-completion detection was a real bug that would have been missed by a naive pass/fail check. We document not just the cost numbers but the failure modes (shape sensitivity, silent non-completion, stall attractor).

3. **Task-shape sensitivity finding with honest disclosure.** Two of three shapes win decisively (implement −53.5%, refactor −45-72%); the third (bugfix) regresses +34% with known root cause. This nuance is more honest than most published agent efficiency papers, which tend to report averages without shape breakdown.

4. **Two-part failure characterization** that distinguishes between:
   - Training-data artifacts in the specialist inner model (stall attractor → Phase 13 fix)
   - Verify-command weakness leading to silent non-completion (warnings → in-place fix)

## What the paper is still missing

Based on the related work review, the paper needs:

1. **More task shapes** — three shapes is thin compared to multi-benchmark papers. CodeAgents uses GAIA + HotpotQA + VirtualHome. MCPVerse has hundreds. AppWorld has 168 tasks. Nebula would benefit from 5-10 shapes minimum (task #15).

2. **Cross-model host validation** — all our data is Sonnet 4.6. Opus 4.6 would confirm the architectural win isn't Sonnet-specific.

3. **Inner-model ablation** — runs at Phase 12 vs stock Qwen3-Coder-30B vs (eventually) Phase 13 would show how much of the win is architectural vs inner-model quality.

4. **Comparison to CodeAgents (or at least, framing against it).** We should cite CodeAgents as the closest magnitude comparison and argue our mechanism is complementary (tool-surface abstraction vs reasoning codification).

5. **Formal cost model.** We report empirical cost deltas but no closed-form model for when delegate wins. Simple model: `host_cost = N_tool_calls × avg_roundtrip_cost + overhead`. Delegate wins when `1 × delegate_roundtrip_cost + delegate_overhead < N × host_roundtrip_cost`. Threshold depends on task shape.

## Action items from this research

- [ ] Add related-work section to the paper citing CodeAgents (2507.03254), AWO (2601.22037), SimPy (2404.16333), Token Sugar (2512.08266).
- [ ] Cite (2512.07497) and (2509.25370) for the bugfix stall pattern framing.
- [ ] Consider running 1-2 tasks on CodeAgents' benchmarks (GAIA, HotpotQA) to produce a directly comparable number.
- [ ] Consider a table explicitly positioning Nebula vs CodeAgents vs AWO on the axes (mechanism / magnitude / benchmark coverage / determinism / novelty).
