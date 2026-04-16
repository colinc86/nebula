# GAIA / CodeAgents head-to-head

This directory holds the direct comparison between the Nebula delegate MCP tool and the CodeAgents architecture (arXiv 2507.03254) on a hand-picked subset of GAIA level-1 tasks.

## Why GAIA

[GAIA](https://huggingface.co/datasets/gaia-benchmark/GAIA) is the benchmark CodeAgents uses for its headline numbers: −55–87% input-token reduction and −41–70% output-token reduction vs a baseline JSON tool-use agent. We want to know whether Nebula's "hide the tool surface behind a single natural-language delegate" produces wins in the same band, in a different band, or not at all, on the same kind of workload.

GAIA is heavy on web-browsing, image parsing, and file-download tasks — none of which match Nebula's tool surface (read_code / patch_code / bash). We therefore restrict to a curated subset of level-1 tasks that are **code-editing-shaped**: tasks where the answer comes from reading or modifying a local file, running a script, or inspecting text. Web/vision tasks are out of scope.

## Subset

See `subset.txt`. 20 level-1 tasks selected by hand-filtering the GAIA validation split for tasks that can be completed with read_file + grep + bash + small edits. The format is one task id per line. Each id maps to a `{id, question, files[], answer}` record in the dataset.

## Status — 2026-04-14

**Infrastructure complete; dataset fetch blocked on HuggingFace credentials.**

GAIA is a gated dataset on HuggingFace (`gaia-benchmark/GAIA`). To fetch it you need:

1. A HuggingFace account that has accepted the GAIA terms (one-click on the dataset page)
2. A HuggingFace token exported as `HF_TOKEN` in the shell
3. The `datasets` Python package installed: `pip install datasets`

Because these are per-user credentials and not present in this repo, Thread D's runs are not part of this session. All scaffolding is in place to run them as soon as credentials are set up.

## How to run (once credentials are configured)

```bash
cd bench/gaia
# 1. Fetch the dataset (one-time; requires HF_TOKEN env var)
pip install datasets
python3 -c "from datasets import load_dataset; load_dataset('gaia-benchmark/GAIA', '2023_level1', split='validation').to_json('gaia_l1.jsonl')"

# 2. Hand-curate subset.txt — pick 20 task_ids from gaia_l1.jsonl that are
# code-editing-shaped (no web, no images, no audio). Selection procedure in
# the comment block of subset.txt.

# 3. Run the subset against {builtins, delegate}
bash run_gaia_subset.sh

# 4. Score the results
python3 score_gaia.py
```

Each task is copied to a fresh `runs/gaia_<id>_<config>/` directory (with any attached files GAIA provides), and `claude --print` is invoked with the same MCP configs used in `bench/mcp_manual_test/` (`empty_mcp.json` for builtins, `delegate_stock_mcp.json` for delegate). The agent's `result` field is the final answer, graded against GAIA's reference answer (exact match, normalized).

## Expected outputs

- `runs/gaia_<id>_<config>/result.json` — `claude --print` output.
- `results.md` — per-task table, aggregate cost/token comparison, pass rate.
- `summary.csv` — machine-readable summary.

## Comparison target

CodeAgents reported numbers (paper Table 2):
- Input token reduction: −55% to −87% across 4 model backends.
- Output token reduction: −41% to −70%.
- Accuracy maintained within ±2pp.

Our headline will answer: **on this subset, does Nebula's single-`code_task` delegate produce cost reductions in the same band, and at what accuracy cost?**
