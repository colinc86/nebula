#!/usr/bin/env python3
"""Submit the multi-task Qwen3-Coder-30B-A3B fine-tune to Together.

Reads bench/finetune/sidecar_poc/data/combined_{train,val}.jsonl, uploads,
launches a QLoRA job. Saves job info to combined_finetune_job.json.

Pre-authorized by user. Estimated cost: $20-50 (30B QLoRA on Together).
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

# Phase 12 used Qwen3-Coder-30B-A3B-Instruct as its base. This is the
# combined model we fine-tune for both inner-agent and sidecar roles.
BASE_MODEL = "Qwen/Qwen3-Coder-30B-A3B-Instruct"
EPOCHS = 3
LR = 1e-4
LORA_R = 16
LORA_ALPHA = 32
SUFFIX = "nebcombined"


def load_api_key() -> str:
    if "TOGETHER_API_KEY" in os.environ:
        return os.environ["TOGETHER_API_KEY"]
    if ENV_FILE.exists():
        for line in ENV_FILE.read_text().splitlines():
            if line.startswith("TOGETHER_API_KEY="):
                return line.split("=", 1)[1].strip().strip('"').strip("'")
    print("TOGETHER_API_KEY not set", file=sys.stderr); sys.exit(1)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--model", default=BASE_MODEL)
    ap.add_argument("--batch-size", type=int, default=2,
                    help="Training batch size (default 2 for 30B-A3B MoE; use 8 for 14B dense)")
    ap.add_argument("--lr", type=float, default=LR,
                    help="Learning rate (default 1e-4 for MoE; 1e-5 is typical for 14B dense)")
    ap.add_argument("--suffix", default=SUFFIX,
                    help="Together job suffix (default nebcombined)")
    args = ap.parse_args()

    train_path = HERE / "data" / "combined_train.jsonl"
    val_path = HERE / "data" / "combined_val.jsonl"
    if not train_path.exists():
        print(f"missing {train_path} — run combine_train.py first", file=sys.stderr)
        sys.exit(1)

    n_train = sum(1 for _ in train_path.open())
    n_val = sum(1 for _ in val_path.open()) if val_path.exists() else 0

    # Rough cost estimate: 30B QLoRA on Together is ~$0.80/1M tokens
    # Each agent trace is ~10-30k tokens, each sidecar triple ~2k tokens
    avg_tokens = 8000
    est_tokens = n_train * avg_tokens * EPOCHS
    est_cost = est_tokens / 1_000_000 * 0.80

    print("=== Combined multi-task fine-tune ===")
    print(f"  Base model : {args.model}")
    print(f"  Train      : {n_train} examples → {train_path}")
    print(f"  Val        : {n_val} examples → {val_path}")
    print(f"  Epochs     : {EPOCHS}")
    print(f"  LoRA r/α   : {LORA_R}/{LORA_ALPHA}")
    print(f"  Batch size : {args.batch_size}")
    print(f"  LR         : {args.lr}")
    print(f"  Est tokens : ~{est_tokens:,}")
    print(f"  Est cost   : ~${est_cost:.2f}")
    print(f"  Suffix     : {args.suffix}")

    if args.dry_run:
        print("Dry run — exiting.")
        return

    api_key = load_api_key()
    sys.path.insert(0, str(REPO / ".venv" / "lib" / "python3.13" / "site-packages"))
    from together import Together  # type: ignore
    client = Together(api_key=api_key)

    print("\n[1/3] Uploading train...")
    tr = client.files.upload(file=str(train_path), purpose="fine-tune")
    print(f"       {tr.id}")
    print("[1/3] Uploading val...")
    vl = client.files.upload(file=str(val_path), purpose="fine-tune")
    print(f"       {vl.id}")

    print("[2/3] Launching fine-tune...")
    job = client.fine_tuning.create(
        training_file=tr.id,
        validation_file=vl.id,
        model=args.model,
        n_epochs=EPOCHS,
        learning_rate=args.lr,
        batch_size=args.batch_size,
        suffix=args.suffix,
        lora=True,
        lora_r=LORA_R,
        lora_alpha=LORA_ALPHA,
        lora_dropout=0.05,
    )
    print(f"       job_id: {job.id}  status: {job.status}")

    info = {
        "job_id": job.id,
        "base_model": args.model,
        "train_file_id": tr.id,
        "val_file_id": vl.id,
        "n_train": n_train,
        "n_val": n_val,
        "epochs": EPOCHS,
        "lr": args.lr,
        "batch_size": args.batch_size,
        "lora_r": LORA_R,
        "lora_alpha": LORA_ALPHA,
        "suffix": args.suffix,
        "started": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    out_path = HERE / f"{args.suffix}_finetune_job.json"
    out_path.write_text(json.dumps(info, indent=2))
    print(f"\nSaved → {out_path}")


if __name__ == "__main__":
    main()
