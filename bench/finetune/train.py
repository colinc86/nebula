#!/usr/bin/env python3
"""Train a Nebula→Python neural refinement model via Together AI.

Uploads training data and launches a fine-tuning job on Together AI's
infrastructure. Much faster than local training and handles GPU allocation.

Usage:
    # Prepare data first:
    python3 prepare_data.py all --count 20000 --output-dir data/

    # Launch training:
    python3 train.py --data data/train.jsonl --val data/val.jsonl

    # Check job status:
    python3 train.py --status JOB_ID

    # With custom model:
    python3 train.py --data data/train.jsonl --model Qwen/Qwen2.5-1.5B --epochs 3
"""

import argparse
import json
import os
import sys
import time
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
REPO_ROOT = SCRIPT_DIR.parents[1]
ENV_FILE = REPO_ROOT / "openai.env"


def load_api_key():
    """Load Together API key from environment or .env file."""
    key = os.environ.get("TOGETHER_API_KEY")
    if key:
        return key

    if ENV_FILE.exists():
        for line in ENV_FILE.read_text().splitlines():
            if line.startswith("TOGETHER_API_KEY="):
                key = line.split("=", 1)[1].strip().strip('"')
                os.environ["TOGETHER_API_KEY"] = key
                return key

    print("Error: TOGETHER_API_KEY not set.")
    print(f"Set it in environment or in {ENV_FILE}")
    sys.exit(1)


def upload_file(client, path, purpose="fine-tune"):
    """Upload a JSONL file to Together AI."""
    print(f"  Uploading {path}...")
    response = client.files.upload(file=path, purpose=purpose)
    file_id = response.id
    print(f"  Uploaded: {file_id}")
    return file_id


def launch_training(args):
    """Upload data and launch a fine-tuning job."""
    api_key = load_api_key()

    try:
        from together import Together
    except ImportError:
        print("Installing together SDK...")
        import subprocess
        subprocess.check_call([sys.executable, "-m", "pip", "install", "together", "-q"])
        from together import Together

    client = Together(api_key=api_key)

    # Count examples
    train_count = sum(1 for _ in open(args.data))
    val_count = sum(1 for _ in open(args.val)) if args.val else 0

    print("=== Nebula Neural Refinement — Together AI ===")
    print()
    print(f"  Model:    {args.model}")
    print(f"  Train:    {train_count} examples")
    print(f"  Val:      {val_count} examples")
    print(f"  Epochs:   {args.epochs}")
    print(f"  LR:       {args.lr}")
    print(f"  LoRA:     rank={args.lora_r}")
    print(f"  Batch:    {args.batch_size}")
    print()

    # Estimate cost
    # Together AI charges ~$0.0004/1K tokens for fine-tuning small models
    # Rough estimate: 20K examples × 500 tokens avg × 3 epochs × $0.0004/1K
    est_tokens = train_count * 500 * args.epochs
    est_cost = est_tokens / 1000 * 0.0004
    print(f"  Est. tokens: {est_tokens:,}")
    print(f"  Est. cost:   ~${est_cost:.2f}")
    print()

    if args.dry_run:
        print("Dry run — exiting.")
        return

    # Upload training file
    print("[1/3] Uploading training data...")
    train_file_id = upload_file(client, args.data)

    val_file_id = None
    if args.val:
        print("[1/3] Uploading validation data...")
        val_file_id = upload_file(client, args.val)

    # Launch fine-tuning job
    print("[2/3] Launching fine-tuning job...")

    job_params = {
        "training_file": train_file_id,
        "model": args.model,
        "n_epochs": args.epochs,
        "learning_rate": args.lr,
        "batch_size": args.batch_size,
        "suffix": args.suffix,
        "lora": True,
        "lora_r": args.lora_r,
        "lora_alpha": args.lora_alpha,
        "lora_dropout": 0.05,
    }

    if val_file_id:
        job_params["validation_file"] = val_file_id

    response = client.fine_tuning.create(**job_params)
    job_id = response.id

    print(f"  Job ID: {job_id}")
    print(f"  Status: {response.status}")
    print()
    print(f"Monitor with: python3 train.py --status {job_id}")
    print(f"Together dashboard: https://api.together.xyz/playground")
    print()

    # Save job info
    job_info = {
        "job_id": job_id,
        "model": args.model,
        "train_file": train_file_id,
        "val_file": val_file_id,
        "train_count": train_count,
        "epochs": args.epochs,
        "lr": args.lr,
        "lora_r": args.lora_r,
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    info_path = SCRIPT_DIR / "last_job.json"
    with open(info_path, "w") as f:
        json.dump(job_info, f, indent=2)
    print(f"Job info saved to {info_path}")

    # Optionally wait for completion
    if args.wait:
        print()
        print("Waiting for job to complete...")
        wait_for_job(client, job_id)


def check_status(args):
    """Check the status of a fine-tuning job."""
    api_key = load_api_key()

    from together import Together
    client = Together(api_key=api_key)

    job_id = args.status

    # If no job ID given, check last job
    if not job_id or job_id == "last":
        info_path = SCRIPT_DIR / "last_job.json"
        if info_path.exists():
            with open(info_path) as f:
                info = json.load(f)
            job_id = info["job_id"]
            print(f"Using last job: {job_id}")
        else:
            print("No job ID provided and no last_job.json found.")
            sys.exit(1)

    response = client.fine_tuning.retrieve(id=job_id)

    print(f"Job ID:     {response.id}")
    print(f"Status:     {response.status}")
    print(f"Model:      {response.model}")

    if hasattr(response, "output_name") and response.output_name:
        print(f"Output:     {response.output_name}")
    if hasattr(response, "training_file"):
        print(f"Train file: {response.training_file}")

    # Show events if available
    if hasattr(response, "events") and response.events:
        print(f"\nRecent events:")
        for event in response.events[-5:]:
            print(f"  {event}")

    return response.status


def wait_for_job(client, job_id, poll_interval=30):
    """Poll until job completes."""
    while True:
        response = client.fine_tuning.retrieve(id=job_id)
        status = response.status

        if status in ("completed", "succeeded"):
            print(f"\nJob completed!")
            if hasattr(response, "output_name") and response.output_name:
                print(f"Fine-tuned model: {response.output_name}")
                print(f"\nTo evaluate:")
                print(f"  python3 eval.py --model {response.output_name} --problems 164")
            return response

        if status in ("failed", "cancelled"):
            print(f"\nJob {status}.")
            if hasattr(response, "error") and response.error:
                print(f"Error: {response.error}")
            return response

        print(f"  Status: {status} (checking again in {poll_interval}s...)")
        time.sleep(poll_interval)


def list_jobs(args):
    """List recent fine-tuning jobs."""
    api_key = load_api_key()
    from together import Together
    client = Together(api_key=api_key)

    response = client.fine_tuning.list()
    print(f"{'ID':<30} {'Model':<35} {'Status':<12}")
    print("-" * 80)
    for job in response.data[:10]:
        model = getattr(job, 'model', 'unknown')
        status = getattr(job, 'status', 'unknown')
        print(f"{job.id:<30} {model:<35} {status:<12}")


def main():
    parser = argparse.ArgumentParser(description="Train Nebula→Python model on Together AI")
    parser.add_argument("--data", help="Training JSONL path")
    parser.add_argument("--val", default=None, help="Validation JSONL path")
    parser.add_argument("--model", default="Qwen/Qwen2.5-1.5B", help="Base model")
    parser.add_argument("--epochs", type=int, default=3, help="Training epochs")
    parser.add_argument("--batch-size", type=int, default=4, help="Batch size")
    parser.add_argument("--lr", type=float, default=2e-5, help="Learning rate")
    parser.add_argument("--lora-r", type=int, default=16, help="LoRA rank")
    parser.add_argument("--lora-alpha", type=int, default=16, help="LoRA alpha")
    parser.add_argument("--suffix", default="nebula-python", help="Model name suffix")
    parser.add_argument("--dry-run", action="store_true", help="Print config and exit")
    parser.add_argument("--wait", action="store_true", help="Wait for job completion")
    parser.add_argument("--status", nargs="?", const="last", help="Check job status")
    parser.add_argument("--list", action="store_true", help="List recent jobs")
    args = parser.parse_args()

    if args.list:
        list_jobs(args)
    elif args.status is not None:
        check_status(args)
    elif args.data:
        launch_training(args)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
