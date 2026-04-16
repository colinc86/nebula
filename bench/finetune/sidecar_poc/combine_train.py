#!/usr/bin/env python3
"""Combine spoofed-mode agent traces + sidecar triples into one
chat-completion JSONL for the multi-task Phase 20 fine-tune.

Inputs:
  - bench/finetune/qwen3_coder_30b/data/spoofed_traces.jsonl  (agent traces)
  - bench/finetune/sidecar_poc/data/sidecar_train.jsonl       (sidecar triples)

Output:
  - bench/finetune/sidecar_poc/data/combined_train.jsonl
  - bench/finetune/sidecar_poc/data/combined_val.jsonl

Filtering:
  - Agent traces: keep only verify_passed=true AND turns ≤ median (trajectory
    efficiency filter — Phase 12 didn't have this, hence the regression).
  - Sidecar triples: keep all (already filtered during synthesis).
  - Strip _meta and other private keys; only emit `messages` lists.

Mix ratio: aim for ~70% agent / ~30% sidecar by example count, weighted to
match expected runtime call frequency (one agent loop has many tool calls,
each potentially routing through the sidecar).
"""

from __future__ import annotations
import json
import random
import statistics
import sys
from pathlib import Path

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent

AGENT_PATH = REPO / "bench/finetune/qwen3_coder_30b/data/spoofed_traces.jsonl"
SIDECAR_PATH = HERE / "data/sidecar_train.jsonl"
SIDECAR_VAL_PATH = HERE / "data/sidecar_val.jsonl"
OUT_TRAIN = HERE / "data/combined_train.jsonl"
OUT_VAL = HERE / "data/combined_val.jsonl"

random.seed(20260415)


def count_turns(rec: dict) -> int:
    """Count assistant turns in the trace. distill.py only writes traces that
    passed verify, so verify_passed is implicit for any record in the file."""
    return sum(1 for m in rec.get("messages", []) if m.get("role") == "assistant")


def strip_private(rec: dict) -> dict:
    return {"messages": rec["messages"]}


def main() -> None:
    if not AGENT_PATH.exists():
        print(f"missing agent traces: {AGENT_PATH}", file=sys.stderr)
        sys.exit(1)
    if not SIDECAR_PATH.exists():
        print(f"missing sidecar train: {SIDECAR_PATH}", file=sys.stderr)
        sys.exit(1)

    # Load agent traces (distill.py only writes verify_passed traces).
    # We use ALL passing traces — the earlier trajectory-efficiency filter
    # cut the training set in half, which for a multi-task run would leave
    # the agent mode undertrained. Better: use quantity and let the model
    # average over the trajectory distribution.
    raw_agents = [json.loads(l) for l in AGENT_PATH.open()]
    print(f"Loaded {len(raw_agents)} agent traces from {AGENT_PATH.name}")
    print(f"  (all are verify_passed by construction)")

    turns_list = [count_turns(r) for r in raw_agents]
    if turns_list:
        print(f"  turns distribution: min={min(turns_list)} median={statistics.median(turns_list):.1f} max={max(turns_list)}")

    # Hold out 10% of agents for val
    random.shuffle(raw_agents)
    n_agent_val = max(1, len(raw_agents) // 10)
    agent_val = raw_agents[:n_agent_val]
    agent_train = raw_agents[n_agent_val:]

    # Load sidecar triples
    sidecar_train_all = [json.loads(l) for l in SIDECAR_PATH.open()]
    sidecar_val_all = [json.loads(l) for l in SIDECAR_VAL_PATH.open()] if SIDECAR_VAL_PATH.exists() else []
    print(f"Loaded {len(sidecar_train_all)} sidecar train + {len(sidecar_val_all)} val from {SIDECAR_PATH.name}")

    # Target 50/50 by example count. Agent traces are larger (~7k tokens
    # each) than sidecar triples (~2k), so 50/50 by count lands roughly
    # 70/30 by token count — the right weighting because the agent task
    # matters more at inference time.
    n_sidecar_target = len(agent_train)
    n_sidecar_target = min(n_sidecar_target, len(sidecar_train_all))
    random.shuffle(sidecar_train_all)
    sidecar_pick = sidecar_train_all[:n_sidecar_target]
    sidecar_val = sidecar_val_all[:max(1, n_agent_val)]

    train_rows = [strip_private(r) for r in agent_train] + [strip_private(r) for r in sidecar_pick]
    random.shuffle(train_rows)

    val_rows = [strip_private(r) for r in agent_val] + [strip_private(r) for r in sidecar_val[:max(1, len(agent_val))]]
    random.shuffle(val_rows)

    OUT_TRAIN.parent.mkdir(parents=True, exist_ok=True)
    with OUT_TRAIN.open("w") as fh:
        for r in train_rows:
            fh.write(json.dumps(r) + "\n")
    with OUT_VAL.open("w") as fh:
        for r in val_rows:
            fh.write(json.dumps(r) + "\n")

    print()
    print(f"Wrote train: {len(train_rows)} rows ({len(agent_train)} agent + {len(sidecar_pick)} sidecar) → {OUT_TRAIN}")
    print(f"Wrote val:   {len(val_rows)} rows ({len(agent_val)} agent + {min(len(agent_val), len(sidecar_val))} sidecar) → {OUT_VAL}")
    ratio = len(agent_train) / max(1, len(train_rows))
    print(f"Train mix: {ratio*100:.0f}% agent / {(1-ratio)*100:.0f}% sidecar")


if __name__ == "__main__":
    main()
