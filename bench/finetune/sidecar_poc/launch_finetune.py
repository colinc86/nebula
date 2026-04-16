#!/usr/bin/env python3
"""Launch the sidecar 7B fine-tune on Together AI.

Reads sidecar_train.jsonl / sidecar_val.jsonl, strips the `_meta` field
(Together's validator rejects unknown top-level keys), uploads, and launches
a QLoRA job on Qwen2.5-Coder-7B-Instruct.
"""

from __future__ import annotations
import argparse
import json
import os
import sys
import time
from pathlib import Path

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
ENV_FILE = REPO / ".env"

BASE_MODEL = "Qwen/Qwen2.5-7B-Instruct"  # Together-supported. Note: not the
# coder variant — Qwen/Qwen2.5-Coder-7B-Instruct is NOT on Together's fine-tune
# list (only the 32B coder is). The non-coder 7B is what the user's prior
# Phase 5 fine-tunes used (see colinc86_4aa1/Qwen2.5-7B-nebula-* models).
EPOCHS = 3
LR = 1e-4
LORA_R = 16
LORA_ALPHA = 16
BATCH_SIZE = 8  # Together's minimum for Qwen2.5-7B-Instruct
SUFFIX = "nebsidecar"


def load_api_key() -> str:
    key = os.environ.get("TOGETHER_API_KEY", "")
    if key:
        return key
    if ENV_FILE.exists():
        for line in ENV_FILE.read_text().splitlines():
            if line.startswith("TOGETHER_API_KEY="):
                key = line.split("=", 1)[1].strip().strip('"').strip("'")
                os.environ["TOGETHER_API_KEY"] = key
                return key
    print("TOGETHER_API_KEY not set", file=sys.stderr)
    sys.exit(1)


def clean_jsonl(in_path: Path, out_path: Path) -> int:
    n = 0
    with in_path.open() as r, out_path.open("w") as w:
        for line in r:
            row = json.loads(line)
            clean = {"messages": row["messages"]}
            w.write(json.dumps(clean) + "\n")
            n += 1
    return n


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true", help="Prep files but don't launch")
    ap.add_argument("--model", default=BASE_MODEL)
    args = ap.parse_args()

    api_key = load_api_key()

    data_dir = HERE / "data"
    train_clean = data_dir / "train_clean.jsonl"
    val_clean = data_dir / "val_clean.jsonl"
    n_train = clean_jsonl(data_dir / "sidecar_train.jsonl", train_clean)
    n_val = clean_jsonl(data_dir / "sidecar_val.jsonl", val_clean)

    # Token estimate: each triple is ~1800 tokens (sys + original + nebula + assistant)
    est_tokens = n_train * 1800 * EPOCHS
    est_cost = est_tokens / 1_000_000 * 0.4  # Together fine-tune pricing ~$0.40/1M for 7B

    print(f"=== Nebula sidecar 7B fine-tune ===")
    print(f"  Base model   : {args.model}")
    print(f"  Train file   : {train_clean}  ({n_train} examples)")
    print(f"  Val file     : {val_clean}  ({n_val} examples)")
    print(f"  Epochs       : {EPOCHS}")
    print(f"  LoRA r/alpha : {LORA_R}/{LORA_ALPHA}")
    print(f"  LR / batch   : {LR} / {BATCH_SIZE}")
    print(f"  Est tokens   : {est_tokens:,}")
    print(f"  Est cost     : ~${est_cost:.2f}")
    print(f"  Suffix       : {SUFFIX}")

    if args.dry_run:
        print("Dry run — not launching.")
        return

    from together import Together  # type: ignore
    client = Together(api_key=api_key)

    print("\n[1/3] Uploading training file...")
    tr = client.files.upload(file=str(train_clean), purpose="fine-tune")
    print(f"       {tr.id}")

    print("[1/3] Uploading validation file...")
    vl = client.files.upload(file=str(val_clean), purpose="fine-tune")
    print(f"       {vl.id}")

    print("[2/3] Launching fine-tune...")
    job = client.fine_tuning.create(
        training_file=tr.id,
        validation_file=vl.id,
        model=args.model,
        n_epochs=EPOCHS,
        learning_rate=LR,
        batch_size=BATCH_SIZE,
        suffix=SUFFIX,
        lora=True,
        lora_r=LORA_R,
        lora_alpha=LORA_ALPHA,
        lora_dropout=0.05,
    )
    print(f"       job_id: {job.id}")
    print(f"       status: {job.status}")

    info = {
        "job_id": job.id,
        "base_model": args.model,
        "train_file_id": tr.id,
        "val_file_id": vl.id,
        "n_train": n_train,
        "n_val": n_val,
        "epochs": EPOCHS,
        "lr": LR,
        "lora_r": LORA_R,
        "lora_alpha": LORA_ALPHA,
        "suffix": SUFFIX,
        "started": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    (HERE / "finetune_job.json").write_text(json.dumps(info, indent=2))
    print(f"\nSaved job info → {HERE / 'finetune_job.json'}")
    print(f"Monitor at : https://api.together.xyz/playground")


if __name__ == "__main__":
    main()
