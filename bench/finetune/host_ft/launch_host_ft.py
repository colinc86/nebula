"""Submit the host fine-tune job to OpenAI.

Mirrors the shape of `bench/finetune/sidecar_poc/launch_combined_finetune.py`:
- Load API key from .env
- Upload train + val files
- Submit fine-tune with suffix
- Poll until done
- Save a metadata JSON to `jobs/`

Pilot default: gpt-4.1-mini-2025-07-18 (~$10). Full run: gpt-4.1 (~$150).
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import sys
import time
from pathlib import Path

from config import FINAL, JOBS_DIR, PILOT_MODEL, FULL_MODEL, ensure_dirs, REPO


ENV_FILE = REPO / ".env"


def load_api_key() -> str:
    if "OPENAI_API_KEY" in os.environ:
        return os.environ["OPENAI_API_KEY"]
    if ENV_FILE.exists():
        for line in ENV_FILE.read_text().splitlines():
            if line.startswith("OPENAI_API_KEY="):
                return line.split("=", 1)[1].strip().strip('"').strip("'")
    print("OPENAI_API_KEY not set", file=sys.stderr)
    sys.exit(1)


def file_sha256(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(64 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--model", default=PILOT_MODEL,
                    help=f"Base model (default: {PILOT_MODEL}). For the full run: {FULL_MODEL}.")
    ap.add_argument("--suffix", default="nebulahost-pilot")
    ap.add_argument("--resume", default=None,
                    help="Skip upload+create, jump straight to polling this job_id.")
    ap.add_argument("--epochs", type=int, default=None,
                    help="Override n_epochs (default: 'auto'). Use 2 to reduce overfit risk.")
    ap.add_argument("--lr-mul", type=float, default=None,
                    help="Override learning_rate_multiplier.")
    args = ap.parse_args()

    ensure_dirs()

    train_path = FINAL["train"]
    val_path = FINAL["val"]
    for p in (train_path, val_path):
        if not p.exists():
            if args.dry_run:
                print(f"(dry-run) {p} does not exist yet — run assemble.py first")
            else:
                print(f"missing {p} — run assemble.py first", file=sys.stderr)
                sys.exit(1)

    n_train = sum(1 for _ in train_path.open()) if train_path.exists() else 0
    n_val = sum(1 for _ in val_path.open()) if val_path.exists() else 0

    print("=== Host fine-tune ===")
    print(f"  Model  : {args.model}")
    print(f"  Suffix : {args.suffix}")
    print(f"  Train  : {n_train} rows  ({train_path})")
    print(f"  Val    : {n_val} rows  ({val_path})")
    if train_path.exists() and val_path.exists():
        print(f"  train.sha256: {file_sha256(train_path)[:16]}…")
        print(f"  val.sha256  : {file_sha256(val_path)[:16]}…")

    # Rough cost estimate. GPT-4.1-mini training is ~$3/M tokens; average row
    # is ~1k tokens; 3 epochs. Real cost printed after job starts.
    avg_tokens_per_row = 1000
    est_train_tokens = n_train * avg_tokens_per_row * 3
    cost_per_mtok = 3.00 if "mini" in args.model else 25.00
    est_cost = est_train_tokens / 1_000_000 * cost_per_mtok
    print(f"  Est cost (rough): ~${est_cost:.2f} (3 epochs × ~{avg_tokens_per_row} tok/row × {cost_per_mtok}/Mtok)")

    if args.dry_run:
        print("Dry run — exiting.")
        return

    api_key = load_api_key()
    sys.path.insert(0, str(REPO / ".venv" / "lib" / "python3.13" / "site-packages"))
    try:
        from openai import OpenAI  # type: ignore
    except ImportError:
        print("`openai` package missing — `pip install openai`", file=sys.stderr)
        sys.exit(1)

    client = OpenAI(api_key=api_key)

    if args.resume:
        job_id = args.resume
        print(f"\n[resume] polling {job_id}")
    else:
        print("\n[1/3] Uploading train...")
        tr = client.files.create(file=train_path.open("rb"), purpose="fine-tune")
        print(f"       {tr.id}")
        print("[1/3] Uploading val...")
        vl = client.files.create(file=val_path.open("rb"), purpose="fine-tune")
        print(f"       {vl.id}")

        print("[2/3] Launching fine-tune...")
        hp = {"n_epochs": "auto", "batch_size": "auto", "learning_rate_multiplier": "auto"}
        if args.epochs is not None:
            hp["n_epochs"] = args.epochs
        if args.lr_mul is not None:
            hp["learning_rate_multiplier"] = args.lr_mul
        print(f"       hyperparameters: {hp}")
        job = client.fine_tuning.jobs.create(
            training_file=tr.id,
            validation_file=vl.id,
            model=args.model,
            suffix=args.suffix,
            hyperparameters=hp,
        )
        job_id = job.id
        print(f"       job_id: {job.id}  status: {job.status}")

    # Poll loop.
    print("[3/3] Polling job status (60s interval)...")
    last_event_id = None
    final = None
    while True:
        status_job = client.fine_tuning.jobs.retrieve(job_id)
        print(f"  {time.strftime('%H:%M:%S')} status={status_job.status}")
        try:
            events = client.fine_tuning.jobs.list_events(job_id, limit=10)
            for ev in reversed(events.data):
                if last_event_id is None or ev.id != last_event_id:
                    print(f"    [{ev.created_at}] {ev.message}")
            if events.data:
                last_event_id = events.data[0].id
        except Exception:
            pass
        if status_job.status in ("succeeded", "failed", "cancelled"):
            final = status_job
            break
        time.sleep(60)

    metadata = {
        "job_id": final.id,
        "base_model": args.model,
        "fine_tuned_model": getattr(final, "fine_tuned_model", None),
        "n_train": n_train,
        "n_val": n_val,
        "status": final.status,
        "hyperparameters": {
            k: getattr(final.hyperparameters, k, None)
            for k in ("n_epochs", "batch_size", "learning_rate_multiplier")
        } if getattr(final, "hyperparameters", None) else None,
        "trained_tokens": getattr(final, "trained_tokens", None),
        "created_at": getattr(final, "created_at", None),
        "finished_at": getattr(final, "finished_at", None),
        "train_sha256": file_sha256(train_path),
        "val_sha256": file_sha256(val_path),
        "started_wall": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    ts = time.strftime("%Y%m%d_%H%M%S")
    out_path = JOBS_DIR / f"host_ft_{args.suffix}_{ts}.json"
    out_path.write_text(json.dumps(metadata, indent=2, default=str))
    print(f"\nSaved → {out_path}")


if __name__ == "__main__":
    main()
