# host_ft — Host-model fine-tune pipeline

Builds a training corpus, launches an OpenAI fine-tune, runs the 5-eval
gate. Target: GPT-4.1 (via GPT-4.1-mini pilot first) that emits Nebula
IL natively, so the Nebula plugin's `write_code` / `patch_code(nebula=true)`
can hit compression on both the read and write paths.

## Directory layout

```
host_ft/
  README.md · config.py · nebula_cli.py · qc.py · assemble.py
  prompt_pools/                  seed prompts for slices 2 + 4, eval pools
  slices/
    slice1_translation.py        HF pairs → (user: convert, assistant: Nebula)
    slice2_instruction.py        Sonnet-answer → compiler-translate → Nebula
    slice3_toolcalls.py          combined_train traces with Nebula tool-arg
    slice4_anchor.py             stock-mini targets (no-Nebula behavior)
  launch_host_ft.py              OpenAI fine-tune submit + poll
  eval_host_ft.py                5-eval gate
  build/                         slice outputs + final train/val/held_out
  eval_results/<run_id>/         per-eval JSON + summary.json
  jobs/                          one metadata JSON per fine-tune run
```

## Phase 0 — prerequisites

- Track 1 (compiler bugfixes) green. `cargo test --release` in `compiler/`.
- `OPENAI_API_KEY` and `ANTHROPIC_API_KEY` in `.env` at repo root.
- `pip install openai anthropic datasets` in your venv.
- HuggingFace auth for the gated `electrocampbell/nebula-8lang-203k` (should
  be public since it's our own dataset — but verify with `huggingface-cli login`
  if slice 1 errors).

## Phase 1 — scaffold (no $ spent)

Already built. Dry-run every script:

```bash
cd bench/finetune/host_ft
python3 slices/slice1_translation.py --dry-run --n 5
python3 slices/slice2_instruction.py --dry-run
python3 slices/slice3_toolcalls.py --dry-run
python3 slices/slice4_anchor.py --dry-run
python3 assemble.py --dry-run
python3 launch_host_ft.py --dry-run
python3 eval_host_ft.py --dry-run
```

## Phase 2 — slice 1 (cheap, local)

```bash
python3 slices/slice1_translation.py          # ~800 rows, no API
```

## Phase 3 — slice 4 (≈$0.06, validates OpenAI)

Requires `prompt_pools/anchor_prompts.jsonl` (250 seeds). Format:
`{"prompt": "...", "source": "humaneval:0|concept:big-o|..."}`

```bash
python3 slices/slice4_anchor.py
```

## Phase 4 — slice 3 (no API, local compiler)

```bash
python3 slices/slice3_toolcalls.py            # transforms 154 existing traces
```

## Phase 5 — slice 2 (≈$17 Sonnet)

Requires `prompt_pools/instruction_prompts.jsonl` (1,800 seeds mixed from
curated_pairs, HumanEval, and synthesized micro-topics). Same row shape
as slice 4's pool.

```bash
python3 slices/slice2_instruction.py --concurrency 6
```

## Phase 6 — assemble + spot-check

```bash
python3 assemble.py --strict-counts
```

Manually spot-check 20 rows per slice from `build/slice*.enriched.jsonl`.

## Phase 7 — pilot launch (≈$10)

```bash
python3 launch_host_ft.py                     # defaults to gpt-4.1-mini
```

Saves metadata → `jobs/host_ft_nebulahost-pilot_<timestamp>.json`.

## Phase 8 — run the 5 evals (≈$3)

```bash
python3 eval_host_ft.py --model ft:gpt-4.1-mini-...:nebulahost-pilot:<id>
```

Each eval writes `eval_results/<run_id>/eval<N>_*.json`. A single
`summary.json` rolls them up with a green / block verdict.

Pass bars:
| Eval | Bar |
|---|---|
| 1. Nebula syntax validity | ≥95% |
| 2. Semantic correctness   | ≥70% |
| 3. Behavior anchor        | avg ≥4.0 / 5 |
| 4. Tool-use fidelity      | ≥90% structurally valid |
| 5. End-to-end round-trip  | ≥7/10 |

## Phase 9 — decision + full fine-tune (≈$150 if green)

Green → relaunch with `--model gpt-4.1-2025-04-14 --suffix nebulahost-v1`.

## Budget & time

See plan §Track 2 for the full accounting. TL;DR: ~$26–30 for the pilot
path, another ~$150 if the pilot passes and we commit to GPT-4.1.

## Honest gotchas

- OpenAI prompt-cache semantics differ from Anthropic's. The cost model
  we're using (Anthropic-cache-aware) may not transfer cleanly to
  OpenAI-hosted inference. Re-measure after the fine-tune is live.
- LiteLLM bridging between Anthropic-format (Claude Code) and OpenAI-
  format (the fine-tuned model) is where most integration bugs land. Plan
  a debug session for the tool-call format translation specifically.
