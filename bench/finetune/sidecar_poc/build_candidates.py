#!/usr/bin/env python3
"""Build 3 candidate training sets with different large-file ratios,
launch all 3 fine-tunes in parallel on Together.

Candidate A: 5% large-file (7 traces)
Candidate B: 10% large-file, short traces only (≤ median turns)
Candidate C: 15% large-file, each trace truncated to ≤20 assistant turns
"""

from __future__ import annotations
import json
import os
import random
import statistics
import sys
import time
from pathlib import Path

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
ENV_FILE = REPO / ".env"

# Data sources
ORIGINAL_TRACES = REPO / "bench/finetune/qwen3_coder_30b/data/spoofed_traces_v1_pre_largefile.jsonl"
LARGEFILE_TRACES = REPO / "bench/finetune/qwen3_coder_30b/data/largefile_traces_v2.jsonl"
SIDECAR_TRAIN = HERE / "data/sidecar_train.jsonl"
SIDECAR_VAL = HERE / "data/sidecar_val.jsonl"

BASE_MODEL = "Qwen/Qwen3-Coder-30B-A3B-Instruct"
EPOCHS = 3
LR = 1e-4
LORA_R = 16
LORA_ALPHA = 32
BATCH = 2

random.seed(20260416)


def count_turns(rec: dict) -> int:
    return sum(1 for m in rec.get("messages", []) if m.get("role") == "assistant")


def truncate_trace(rec: dict, max_assistant_turns: int) -> dict:
    """Keep only the first max_assistant_turns assistant messages (and their
    surrounding tool/user messages)."""
    msgs = rec["messages"]
    out = []
    asst_count = 0
    for m in msgs:
        if m.get("role") == "assistant":
            asst_count += 1
            if asst_count > max_assistant_turns:
                break
        out.append(m)
    result = dict(rec)
    result["messages"] = out
    return result


def strip_private(rec: dict) -> dict:
    return {"messages": rec["messages"]}


def load_api_key() -> str:
    if "TOGETHER_API_KEY" in os.environ:
        return os.environ["TOGETHER_API_KEY"]
    for line in ENV_FILE.read_text().splitlines():
        if line.startswith("TOGETHER_API_KEY="):
            return line.split("=", 1)[1].strip().strip('"').strip("'")
    raise RuntimeError("TOGETHER_API_KEY not set")


def build_candidate(name: str, original: list, largefile_picks: list, sidecar: list, sidecar_val: list, out_dir: Path) -> tuple[Path, Path]:
    """Build train + val JSONL for one candidate. Returns (train_path, val_path)."""
    out_dir.mkdir(parents=True, exist_ok=True)

    # Agent = original + largefile picks
    agent_all = original + largefile_picks
    random.shuffle(agent_all)
    n_val = max(1, len(agent_all) // 10)
    agent_val = agent_all[:n_val]
    agent_train = agent_all[n_val:]

    # Sidecar: match agent count 1:1
    sc = list(sidecar)
    random.shuffle(sc)
    sc_pick = sc[:len(agent_train)]
    sc_val_pick = list(sidecar_val)[:n_val]

    train = [strip_private(r) for r in agent_train] + [strip_private(r) for r in sc_pick]
    val = [strip_private(r) for r in agent_val] + [strip_private(r) for r in sc_val_pick]
    random.shuffle(train)
    random.shuffle(val)

    train_path = out_dir / f"{name}_train.jsonl"
    val_path = out_dir / f"{name}_val.jsonl"
    with train_path.open("w") as f:
        for r in train:
            f.write(json.dumps(r) + "\n")
    with val_path.open("w") as f:
        for r in val:
            f.write(json.dumps(r) + "\n")

    lf_count = len(largefile_picks)
    total_agent = len(agent_train)
    print(f"  {name}: {len(train)} train ({total_agent} agent [{lf_count} largefile] + {len(sc_pick)} sidecar), {len(val)} val")
    return train_path, val_path


def launch_finetune(api_key: str, name: str, train_path: Path, val_path: Path) -> str:
    sys.path.insert(0, str(REPO / ".venv" / "lib" / "python3.13" / "site-packages"))
    from together import Together

    client = Together(api_key=api_key)
    tr = client.files.upload(file=str(train_path), purpose="fine-tune")
    vl = client.files.upload(file=str(val_path), purpose="fine-tune")

    job = client.fine_tuning.create(
        training_file=tr.id,
        validation_file=vl.id,
        model=BASE_MODEL,
        n_epochs=EPOCHS,
        learning_rate=LR,
        batch_size=BATCH,
        suffix=f"neb{name}",
        lora=True,
        lora_r=LORA_R,
        lora_alpha=LORA_ALPHA,
        lora_dropout=0.05,
    )
    print(f"  {name}: job={job.id} status={job.status}")
    return job.id


def main():
    api_key = load_api_key()

    # Load data
    original = [json.loads(l) for l in ORIGINAL_TRACES.open()]
    largefile = [json.loads(l) for l in LARGEFILE_TRACES.open()]
    sidecar = [json.loads(l) for l in SIDECAR_TRAIN.open()]
    sidecar_val = [json.loads(l) for l in SIDECAR_VAL.open()] if SIDECAR_VAL.exists() else []

    print(f"Original traces: {len(original)}")
    print(f"Large-file traces: {len(largefile)}")
    lf_turns = [count_turns(r) for r in largefile]
    median_lf_turns = statistics.median(lf_turns) if lf_turns else 0
    print(f"  turns: min={min(lf_turns)} median={median_lf_turns:.0f} max={max(lf_turns)}")
    print(f"Sidecar train: {len(sidecar)}, val: {len(sidecar_val)}")
    print()

    out_dir = HERE / "data" / "candidates"
    out_dir.mkdir(parents=True, exist_ok=True)
    jobs = {}

    # Candidate A: 5% large-file (7 traces, random sample)
    random.shuffle(largefile)
    a_picks = largefile[:7]
    print("=== Candidate A (5% large-file) ===")
    a_train, a_val = build_candidate("candA", original, a_picks, sidecar, sidecar_val, out_dir)

    # Candidate B: 10% large-file, short traces only (≤ median turns)
    short_lf = [r for r in largefile if count_turns(r) <= median_lf_turns]
    random.shuffle(short_lf)
    b_picks = short_lf[:14]
    print("=== Candidate B (10% large-file, short only) ===")
    b_train, b_val = build_candidate("candB", original, b_picks, sidecar, sidecar_val, out_dir)

    # Candidate C: 15% large-file, token-capped (each trace ≤20 assistant turns)
    c_all_lf = [truncate_trace(r, 20) for r in largefile]
    random.shuffle(c_all_lf)
    c_picks = c_all_lf[:22]
    print("=== Candidate C (15% large-file, truncated to ≤20 turns) ===")
    c_train, c_val = build_candidate("candC", original, c_picks, sidecar, sidecar_val, out_dir)

    # Launch all 3 fine-tunes
    print("\n=== Launching fine-tunes ===")
    for name, train_p, val_p in [("candA", a_train, a_val), ("candB", b_train, b_val), ("candC", c_train, c_val)]:
        job_id = launch_finetune(api_key, name, train_p, val_p)
        jobs[name] = job_id

    info = {
        "candidates": jobs,
        "base_model": BASE_MODEL,
        "launched": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    info_path = HERE / "candidate_jobs.json"
    info_path.write_text(json.dumps(info, indent=2))
    print(f"\nSaved job info → {info_path}")
    print(json.dumps(jobs, indent=2))


if __name__ == "__main__":
    main()
