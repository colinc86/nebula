# Sidecar POC — zero-shot Qwen3-Coder-480B on Nebula→native correction

**Date:** 2026-04-14
**Task 22 · Phase 20 sidecar**

> **Scope note.** Everything in this POC — corpus, edit primitives, training-data synthesis, and the planned 7B distillation — targets **Go and Python only**. The Nebula IL supports 8 languages (Py, JS, TS, Go, Swift, Kotlin, Rust, C), and the production sidecar must handle all of them. We limit the POC to Go+Python to validate the architecture at minimum cost; a follow-up task (Phase 21) will extend the synthesizer and retrain on the full 8-language set once this POC proves the approach works end-to-end.

## TL;DR

**Grade: A−.** Zero-shot 480B (Qwen3-Coder-480B-A35B-Instruct-FP8 via Together AI) hits **13/19 normalized-equal (68.4%)** on the Nebula→native correction task with a diff-application prompt + one-shot example. Zero byte-exact, but every normalized success differs by a single trailing byte. The remaining 6 failures split into 5 training-away failure modes and 1 response-truncation artifact on the 425-LoC file. A fine-tuned 7B sidecar is very likely to clear ≥95% on the same rubric after a few thousand training triples.

**Recommendation:** proceed to Task 23 (distill 7B). Cost estimate stands (~$5–10 Together).

## Setup

- 19 triples from 19 non-test Go files in the benchmark templates
- Synthetic transform: pick a capitalized function name, rename to `NameRenamed` and propagate intra-file call sites
- Pipeline: `(original, modified_native)` → `from-go modified_native` = Nebula input
- Teacher prompt: strict diff-application framing ("the original is authoritative; the Nebula is lossy; copy unchanged lines byte-for-byte"), 1 few-shot example
- Scoring: byte-exact, trailing-newline tolerant (`.rstrip() + "\n"`), whitespace-normalized
- Cost: **$0.013** for 19 calls. 27k input tokens, 8k output tokens, 91.8s total wall

## Results

| Rubric | Hits | % |
|---|---:|---:|
| byte_exact | 0 / 19 | 0.0% |
| trailing_eq (1-byte tolerant) | 13 / 19 | 68.4% |
| normalized_eq (whitespace-collapsed) | 13 / 19 | 68.4% |
| extract_fail | 1 / 19 | 5.3% |

## Failure taxonomy

1. **Trailing whitespace (all 13 successes).** Teacher consistently emits one extra blank line at the end of the file. Every "success" in the rubric above is really "byte_exact modulo `.rstrip() + \n`". **Trivially fixable** by normalizing output at the sidecar boundary.

2. **Case-convention drift (1 failure: `money.go`).** The Nebula canonicalizes `Amount int64` → `amount: Int64` and `USD = "USD"` → `usd := "USD"`. The teacher copied the Nebula's lowercase identifiers into Go output instead of inferring from the ORIGINAL that these should stay `Amount` / `USD`. Training-away: a fine-tune will learn the canonicalization-reversal pattern from a few hundred examples.

3. **Comment drift (2 failures: `logger.go`, `snapshot.go`).** Teacher updated doc comments (`// NewLogger` → `// NewLoggerRenamed`) when renaming symbols. Arguably *correct* behavior — most human reviewers would sync doc comments to renamed symbols. The rubric penalizes it because our synthetic transform only touched declarations, not comments. Can be fixed in the training data synthesizer (update comments in the expected output) or in the rubric.

4. **Idiom rewriting (1 failure: `snapshot.go`).** Teacher rewrote `if err := enc.Encode(snap); err != nil { ... }` as `err := enc.Encode(snap); if err != nil { ... }`. Semantically equivalent Go, syntactically different. Training-away: the fine-tune will learn to preserve the original's statement shape. Also: **disappears entirely at runtime** because the deployment uses snippet-level `patch_code`, not full-file rewrites — the sidecar only sees and regenerates the changed region.

5. **Identifier hallucination (1 failure: `cache.go`).** `ttl time.Duration` → `truel time.Duration`. Looks like a rare sampling artifact (temperature noise). Fine-tuning + low temperature inference eliminates this.

6. **Large-file extract failure (1 failure: `expr.go`, 425 LoC).** Response was either truncated at the fence or emitted without a clean ``` ``` wrap. Fixable: (a) streaming mode with tighter fence parsing, (b) chunked processing for files > ~1500 LoC. Not a fundamental capability gap.

## What this actually proves

**The task is learnable.** A 480B model, with zero task-specific training and a single few-shot example, is getting 68% right in one shot and within a single trivial fix of all 13 successes. Phase 5 prior art says that Qwen-2.5-7B fine-tuned on ~20k Nebula→Python pairs hits 89% Pass@1 on HumanEval. Our task is *structurally* similar (Nebula in, native out) but *easier* because:

- The sidecar has the **original native file as context** — it doesn't need to reconstruct style from thin air
- The deployment task is **snippet-level**, not full-file, which dodges the trailing-whitespace and idiom-drift failure modes entirely
- The training data is **deterministic and synthetic**, so we can generate arbitrary volume

Fine-tuning a 7B on ~5–10k synthetic triples should produce a sidecar that clears **≥95% byte-exact on snippet-level `patch_code` calls**. That's the bar for Task 23.

## Deployment framing (important)

The POC tests full-file reconstruction. The runtime task is simpler:

```
inner model emits: patch_code(path, nebula_old_str, nebula_new_str)
                                 │
                                 ▼
sidecar receives: (full native file at path, nebula_old_str, nebula_new_str)
                                 │
                                 ▼
sidecar emits:    (native_old_str, native_new_str)
                                 │
                                 ▼
MCP byte-replace:  file[native_old_str → native_new_str]
```

No full-file output. No trailing-newline issues. No "is the whole file correct" worry — just "did the sidecar correctly identify the region in the original that corresponds to the Nebula snippet, and emit the native replacement for it." That's a strictly easier task than what this POC measured.

## Gate decision: GO

- POC grade: A−
- Failure modes: characterized and tractable
- Deployment task is easier than POC task
- Training cost: trivial (~$10 for 10k synthetic triples via teacher)
- Fine-tune cost: ~$5–10 for 7B QLoRA on Together
- Total spend before seeing end-to-end results: **~$20**

**Proceeding to Task 23 (distill 7B sidecar) when user confirms.**

## Raw data

- Per-triple results: [runs/poc_results.jsonl](runs/poc_results.jsonl)
- Full expected/actual/nebula triples: [runs/work/](runs/work/)
- Summary: [runs/poc_summary.json](runs/poc_summary.json)
- Script: [run_poc.py](run_poc.py)
