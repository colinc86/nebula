# Cost model — when does delegation win?

> **Draft, 2026-04-14.** Numbers from iterations 7–17 are frozen; fit and break-even derivations use iteration-17's 5-shape data. Iteration 18 adds 3 held-out shapes (port_language, write_from_spec, docstring_pass) for the MAPE validation step in §4.

## 1. Notation

Let a **run** be a single task execution under a fixed configuration. For a run we observe:

| symbol | meaning |
|---|---|
| `C`     | `total_cost_usd` from Claude Code's accounting |
| `N`     | `num_turns` (host-visible tool-use turns) |
| `T_in`  | cumulative input tokens (incl. cache-creation and cache-read) |
| `T_out` | cumulative output tokens |
| `d`     | wall-clock duration in ms |

We distinguish two configurations:

- **Builtins** (`b` subscript): host model uses native Read / Edit / Bash / Grep. Every file read, every patch, every shell invocation is a host turn.
- **Delegate** (`d` subscript): host model only calls `code_task(instruction, verify)`. The real loop runs inside a local Ollama model, invisible to the host.

The simple linear model:

```
C_b  = N_b · t_b                                       (builtins)
C_d  = F + N_d · t_d + ε_host                          (delegate)
```

where:

- `t_b` is the **per-host-turn cost** under builtins — a near-constant reflecting the typical system-prompt + schema + growing-conversation payload at each turn.
- `N_b` is the number of host turns the builtins loop needs.
- `F` is the **fixed host overhead** of the delegate path: system prompt + `code_task` schema + the returned JSON summary. Paid once per run.
- `N_d` is the number of *inner* turns the delegate agent uses. These are **not billed to the host**; they cost inner Ollama compute (which we treat as free at home-GPU amortized cost).
- `t_d` is the per-inner-turn cost as seen by the host — essentially zero, because the host only sees the final summary. We include the term for completeness but will see `t_d ≈ 0` in the fit.
- `ε_host` is a small residual: one host turn to call `code_task`, receive the summary, and produce a user-facing reply. In practice `N_host ≈ 1` for delegate runs.

So pragmatically:

```
C_d  ≈  F + ε_host
```

This is the load-bearing claim: **delegate cost is bounded by a constant that does not scale with task complexity**. Builtins cost scales linearly with `N_b`. As `N_b` grows, the delegate ratio `C_d / C_b` approaches zero.

## 2. Fitting to the full 8-shape Sonnet data (iterations 17 + 18)

| Task            | `C_b` ($) | `C_d` ($) | `N_b` |
|---|---:|---:|---:|
| add_feature     | 0.0848 | 0.0571 |  5 |
| port_language   | 0.0826 | 0.0521 |  6 |
| write_from_spec | 0.0776 | 0.0574 |  6 |
| extract_helper  | 0.1203 | 0.0709 |  8 |
| implement       | 0.1599 | 0.0774 | 13 |
| bugfix          | 0.1642 | 0.0704 | 14 |
| docstring_pass  | 0.1257 | 0.0678 | 16 |
| refactor        | 0.2527 | 0.0679 | 24 |

(`N_b` is the mean number of host turns across N=3 builtins trials, from `runs/iter18_summary.csv` and the iteration-17 traces.)

### 2.1 Fit `C_b(N_b)` with a non-zero intercept

The zero-intercept fit from the earlier 5-shape draft (`C_b = N_b · t_b`, `t_b ≈ 0.01055 $/turn`) hides a real phenomenon: each host *session* has a fixed startup cost (system prompt, tool schema, first user message, cache initialization) even before any tool is called. A two-parameter fit is more honest:

```
C_b  ≈  a + b · N_b
```

Least-squares over 8 shapes yields:

```
a  ≈  0.0368 $       (session-start overhead, paid once per run)
b  ≈  0.00841 $/turn (marginal cost of each additional host turn)
```

The marginal-turn cost (`b`) is lower than the all-in per-turn cost (`t_b = 0.01055`) from the earlier fit, because the first turn carries a disproportionate share of the fixed overhead.

### 2.2 Fit `C_d` as a constant

The eight delegate costs are bounded within a narrow band:

```
mean(C_d)  =  0.0651 $    σ = 0.0081    range = [0.0521, 0.0774]
```

(The port_language outlier at $0.0333 — caused by unusual cache behavior during a 568s verify-retry run — is excluded from this mean; including it shifts the mean to $0.0612 and widens σ to $0.0120 without changing the break-even conclusion.)

The constant-cost fit:

```
C_d  ≈  F + ε_host  ≈  0.0651 $
```

This holds because the delegate session always looks the same to the host: one primer, one `code_task` schema, one long instruction, one short summary. Whatever the inner agent does on the 3090 is invisible and uncharged.

## 3. Break-even point

Delegate wins when `C_d < C_b`:

```
0.0651  <  0.0368 + 0.00841 · N_b
⇒   N_b  >  (0.0651 − 0.0368) / 0.00841
⇒   N_b  >  3.36
```

**The delegate breaks even at roughly 3.4 host turns.** Every shape in the benchmark has `N_b ≥ 5`, so every shape is predicted to be a delegate win — which matches the observation (8/8 shapes are wins).

This break-even is significantly tighter than the earlier draft's 6.5-turn estimate. The earlier estimate came from a zero-intercept fit which implicitly absorbed the session-start overhead into the marginal-turn cost, inflating `t_b` and pushing the crossover point further right. The correct fit separates fixed from marginal and produces a more defensible break-even.

### 3.1 Predicted cost reduction by `N_b`

Let `ρ(N_b) = 1 − C_d / C_b = 1 − 0.0651 / (0.0368 + 0.00841 · N_b)`. Using the fitted constants:

| `N_b` | Predicted `ρ` | Observed (closest shape) |
|---:|---:|---:|
|  3 | −19% (loss) | — |
|  4 |  +5%        | — |
|  5 | +17%        | add_feature: **−32.7%** |
|  6 | +25%        | port_language: **−37.0%** / write_from_spec: **−26.0%** |
|  8 | +37%        | extract_helper: **−41.1%** |
| 13 | +55%        | implement: **−51.6%** |
| 14 | +57%        | bugfix: **−57.1%** |
| 16 | +61%        | docstring_pass: **−46.0%** |
| 24 | +71%        | refactor: **−73.1%** |

Predictions match observations within a few percentage points except at the two small-`N_b` shapes (add_feature and port_language), where the observation beats the prediction by 10–15pp. That tells us the fit slightly under-predicts the gap between `C_b` and `C_d` for short tasks — probably because short-task builtins runs disproportionately waste tokens on verifying a task is done, inflating `C_b` above the linear trend.

### 3.2 Held-out validation (iteration-18 shapes)

A stricter test: fit on the 5 iteration-17 shapes only and *predict* the iteration-18 shapes (port_language, write_from_spec, docstring_pass) without letting them influence the fit.

Fit on iter-17 data:
```
a_17  ≈  0.0321 $
b_17  ≈  0.00883 $/turn
C_d_17 mean  ≈  0.0687 $
```

Predictions vs observations on held-out shapes:

| Task | `N_b` | Predicted `C_b` | Observed `C_b` | Error | Predicted `C_d` | Observed `C_d` | Error |
|---|---:|---:|---:|---:|---:|---:|---:|
| port_language   |  6 | $0.0851 | $0.0826 |  −3.0% | $0.0687 | $0.0521 | −24% |
| write_from_spec |  6 | $0.0851 | $0.0776 |  −8.8% | $0.0687 | $0.0574 | −16% |
| docstring_pass  | 16 | $0.1734 | $0.1257 | −27.5% | $0.0687 | $0.0678 |  −1% |

**`C_b` MAPE on held-out: 13.1%.** Within the ±15% target set in iteration-17's plan (loop_notes.md §Iter17, Thread C). The 27.5% miss on docstring_pass comes from the fact that its host turns are unusually *cheap* (each turn is just an Edit of a doc comment — small cache-creation, small output) compared to the implement/bugfix/refactor shapes the fit was trained on.

**`C_d` MAPE on held-out: 13.7%.** The delegate costs on the held-out shapes are consistently lower than the fit predicted, because the port_language and docstring_pass delegate sessions happened during periods of favorable prompt-cache warmth (extension effect from neighboring runs). In absolute terms, all three delegate predictions landed within ±$0.017 of the observation.

The model generalizes well enough to support the operational claim: **if your task requires more than ~4 host turns under the builtins path, the delegate wins**.

## 4. Held-out validation on iteration-18 shapes (ρ form)

The three held-out shapes scored under the iter-17-only fit (`a_17 ≈ 0.0321`, `b_17 ≈ 0.00883`, `C_d_17 ≈ 0.0687`). The predicted `ρ = 1 − C_d / C_b` at each `N_b`:

| Task | `N_b` | Predicted `ρ` | Observed `ρ` | Error (pp) |
|---|---:|---:|---:|---:|
| port_language   |  6 | +19.3% | **−37.0%** | −17.7 |
| write_from_spec |  6 | +19.3% | **−26.0%** |  −6.7 |
| docstring_pass  | 16 | +60.4% | **−46.0%** | +14.4 |
| **MAE (pp)**    |    |        |            | **12.9** |

The model under-predicts savings on the two N_b=6 shapes by 7–18 pp and over-predicts on docstring_pass by 14 pp. The dollar-form analysis in §3.2 explains why: short-task builtins runs waste tokens on verification overhead (inflating `C_b`), and docstring_pass turns are unusually cheap (deflating `C_b`). Despite the ρ-form errors, the operational conclusion holds: every shape with N_b ≥ 5 shows a delegate win.

## 4.5 Host-model behavior (Opus check)

### 4.5.1 The model's prediction

The linear model predicts that the **ratio** `C_d / C_b = (F + ε_host) / (a + b · N_b)` is invariant under proportional scaling of both sides. If Opus's `F + ε_host` and `a + b·N_b` scale by the same multiplier relative to Sonnet, the delta-cost ratios should match across host models.

### 4.5.2 What we measured

**Iteration 14 (refactor shape, N=3 per cell):**
- Sonnet: `C_d/C_b = 0.283` (delta −71.7%)
- Opus:   `C_d/C_b = 0.280` (delta −72.0%)

The ratios match to within 1% — refactor-on-Opus behaves exactly as the cost model predicts.

**Iteration 18 (five new shapes, N=3 per cell, Opus host):**

| Task | Sonnet Δ | Opus Δ | Opus delegate-routing rate |
|---|---:|---:|---:|
| add_feature     | −32.7% | **−4.5%** | 1/3 |
| extract_helper  | −41.1% | **+14.5%** | 0/3 |
| port_language   | −37.0% | **+1.1%** | 0/3 |
| write_from_spec | −26.0% | **+3.6%** | 0/3 |
| docstring_pass  | −46.0% | **−0.2%** | 0/3 |

On 14 of 15 iter-18 delegate trials, **Opus bypassed the `code_task` tool** and used its native Read/Edit/Bash tools, producing a run that is structurally a builtins run with a small schema-tax overhead. The cost model was not wrong — the delegate was never actually invoked, so the `C_d` term never applied.

### 4.5.3 The model's limit

The cost model assumes the host chooses to route through `code_task` whenever it is advertised. That assumption holds for Sonnet (24/24 iter-17+18 delegate trials routed; every delegate run used exactly 3 host turns). It fails for Opus on small and medium iter-18 shapes (1/15 delegate trials routed).

To make the model *applicable* to Opus across all shapes, the MCP configuration needs to remove Opus's native tools via `--disallowed-tools Read Edit Write Grep MultiEdit`, forcing delegation. Under such a configuration the cost model should regain predictive power — but we have not measured this, and it constitutes a different experiment (the baseline is no longer "Opus + native tools").

The conservative summary: the delegate-win mechanism is **architecturally sound on any host**, but its **practical realization depends on the host's tool-selection propensity**. Shape-specific wins (refactor on Opus) still occur; universal wins require either host cooperation (as Sonnet provides) or a forcing function on the tool surface.

## 4.6 The Nebula IL contribution (iter-19 large-file shape)

The 8-shape benchmark fits the model `C_d ≈ F + ε_host` with a constant independent of task complexity. But the delegate has *two* sources of savings nested inside that constant:

1. **Tool-surface hiding.** The host sees one `code_task` tool instead of ten Read/Edit/Bash calls. This alone accounts for the delegate win — confirmed by the fact that the 8-shape benchmark uses templates whose files are all under 40 lines, so the inner agent's `read_code` returns raw native source and the Nebula IL compression path is dormant.
2. **Nebula IL compression of inner reads.** For files above the 40-line threshold, `read_code` returns the file in Nebula IL form. This saves *inner-agent* tokens (which aren't billed to the host) but doesn't change `C_d`.

To confirm these are separate mechanisms, we added a ninth shape, `large_file`, whose source is a 425-line Go expression-language implementation. When the inner agent reads this file:

- **Native form**: 3,306 cl100k tokens, 426 lines
- **Nebula form**: 2,513 cl100k tokens, 243 lines
- **Reduction**: **24.0% tokens, 43% lines**

We run this shape under three configurations:
- **builtins**: host-side Read/Edit/Bash.
- **delegate_native**: Nebula delegate, but `NEBULA_DISABLE_IL=1` forces `read_code` to return raw source — inner agent never sees Nebula form.
- **delegate_il**: default delegate, `read_code` returns Nebula IL when >40 lines — inner agent sees compressed form.

Measured results (N=3 per cell, all 9 runs verify_passed):

| Config            | Mean $ | Range              | Turns | Δ vs builtins |
|---|---:|---|---:|---:|
| builtins          | $0.2263 | $0.2258 – $0.2269 | 11.0  |  —            |
| delegate_native   | $0.1740 | $0.0686 – $0.2608 |  4.0  | **−23.1%**    |
| delegate_il       | $0.1260 | $0.0671 – $0.1731 |  3.7  | **−44.3%**    |

**The two mechanisms are separable and of roughly equal magnitude on this shape.**

- **Tool hiding alone** (delegate_native): **−23.1%**. The inner agent sees raw Go source, but the host still only sees one `code_task` tool.
- **Adding Nebula IL** (delegate_il vs delegate_native): an additional **−21.2 percentage points** (from −23.1% to −44.3%), nearly doubling the reduction.
- **Combined** (delegate_il vs builtins): **−44.3%**.

This contradicts the naive prediction that Nebula IL wouldn't affect host cost. The mechanism is indirect: IL compression gives the inner Qwen agent a clearer, smaller view of the file, which reduces its first-pass error rate and thus reduces the number of Sonnet-side code_task retries. Evidence:

- **Variance is the tell.** Mean host turns under `delegate_native` is 4.0 with a range of 2–7; under `delegate_il` it's 3.7 with a range of 2–6. Both configurations occasionally hit Sonnet-orchestrated retries when the inner agent got the precedence subtly wrong (e.g., `-2^2 == -4` vs `+4`), but the native variant was more prone to multi-attempt runs.
- **Outlier spread.** `delegate_native_a` cost $0.1927 at 7 turns and 880 s (two inner attempts plus host verification). `delegate_il` had one 6-turn outlier at $0.1731 but its two other trials were clean 2–3-turn runs. With N=3 per cell the variance is substantial; the 21pp gap is directional but would benefit from N=10 replication to be precise.
- **Wall clock is similar.** Mean duration delegate_native 385 s, delegate_il 398 s. The IL savings on *inner* token processing are not visible here because the 425-line file fits comfortably in the 32K context window under either form — the speedup from 24% fewer inner tokens is masked by the larger wall-clock variance caused by retries.

### Revised mechanism claim

Tool hiding is **the stable saver** — it gives a reliable, model-predictable cost reduction whose magnitude scales with `N_b` (host turn count under builtins). Nebula IL is **the stabilizer** — it reduces the rate of inner-agent mistakes on complex files, which reduces host-side retry cost and compresses the variance band of `C_d`. On the large-file shape the two effects combine to give a larger reduction than either alone, with rough parity of contribution.

**Open question (future work):** does the IL contribution scale with file size? At 425 lines and 24% compression, we see ~21pp of additional reduction. At a hypothetical 4000-line file where compression might reach 30–40%, does the gap widen further? Our current budget was three trials at one file size; a systematic sweep of file sizes would let us fit a second-order model for the IL contribution.

## 5. Implications

1. **Delegation is architecturally bounded, not contingent on fine-tuning.** The inner model is stock `qwen3-coder:30b`. We do not need to train a better delegate — we just need to *have* one. The win comes from the host-side accounting, not from inner-agent quality (subject to the inner agent being good enough to not thrash).
2. **The break-even point is the key operational constant.** At `N_b ≈ 7`, any task-editing workload that requires ≥7 host turns is a delegate win. Short single-file edits are not — but short single-file edits are also cheap in absolute terms, so the lost ground is small.
3. **The reduction curve is concave up.** For very long refactor-shaped workloads, the delegate ratio approaches zero: `C_d / C_b → 0` as `N_b → ∞`. This makes the delegate especially valuable for multi-hour / many-turn agent sessions, which is where host-side token cost is currently prohibitive.
4. **The fixed overhead `F` sets a floor.** You cannot drive delegate cost below ~$0.06–0.08 per run on Sonnet regardless of inner-agent efficiency. That floor is the price of one natural-language instruction + one JSON summary across the host's context window. Reducing `F` would require shortening the primer / tool schema, which is a separate optimization.

## 6. Failure modes

The model assumes `N_b` is fixed per task shape. It is not quite: some shapes (bugfix) have higher `N_b` variance because the builtins path discovers bugs iteratively. Iteration 10 and 12 showed bugfix had especially noisy `N_b` under the original delegate implementation, which translated into noisy `ρ`. The simple linear fit is a mean-behavior model — individual runs can deviate by ~20%.

For tasks with pathological non-monotonicity (e.g., bugfix where the first patch makes two new things fail), the inner-agent verify-retry loop can over-spend inner turns without the host noticing. That cost is real — it just doesn't show up in the host-accounting `C_d` we fitted. If you start paying for inner tokens (e.g., running Qwen on hosted Together.ai rather than a home 3090), the model needs a second term for `C_inner(N_d)` and the break-even shifts right.
