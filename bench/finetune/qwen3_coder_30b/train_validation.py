#!/usr/bin/env python3
"""Validation training run for nebula-host-30b — small LoRA fine-tune of
Qwen3-Coder-30B-A3B-Instruct on the distilled agentic dataset.

This is the validation pass (Phase 4 of the plan). It exists to:
1. Confirm the entire training pipeline works end-to-end (upload, submit,
   poll, retrieve checkpoint).
2. Validate that the dataset format is accepted by Together AI.
3. Produce a small adapter we can deploy and benchmark in Phase 5 to confirm
   the dataset actually teaches the right behaviors.

PER THE TRAINING-COST APPROVAL GATE: this script will NOT submit a job unless
--confirm is passed. The default behavior is a dry-run that prints the brief
and validates the dataset.

Usage:
    # Step 1: dataset validation + brief
    python3 train_validation.py --data data/validation_train.jsonl

    # Step 2: after explicit user approval
    python3 train_validation.py --data data/validation_train.jsonl --confirm
"""

import argparse
import json
import os
import sys
import time
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
REPO_ROOT = Path(__file__).resolve().parents[3]
# Look in both legacy `openai.env` and the renamed `.env`
ENV_FILES = [REPO_ROOT / ".env", REPO_ROOT / "openai.env"]

# ── Defaults ────────────────────────────────────────────────────────────────

BASE_MODEL = "Qwen/Qwen3-Coder-30B-A3B-Instruct"
SUFFIX = "nebula-host-validation"

# LoRA hyperparameters
LORA_R = 16
LORA_ALPHA = 16
LORA_DROPOUT = 0.05

# LoRA target modules. The HF-canonical names (q_proj, k_proj, v_proj, o_proj,
# gate_proj, up_proj, down_proj) are NOT what Together's API expects for
# Qwen3-Coder-30B-A3B-Instruct — its first error told us `gate_proj` is unknown.
# Falling back to attention-only on the first attempt; if that's also rejected,
# we'll punt to Together's default `all-linear` and accept the MoE-router risk.
LORA_TRAINABLE_MODULES = "q_proj,k_proj,v_proj,o_proj"

# Training schedule (smaller LR than 7B/14B because more params; 2 epochs for validation)
LEARNING_RATE = 5e-5
BATCH_SIZE = 2  # Together caps Qwen3-Coder-30B-A3B at batch_size=2
N_EPOCHS = 2
WARMUP_RATIO = 0.05
SEQ_LENGTH = 8192


def load_api_key():
    key = os.environ.get("TOGETHER_API_KEY")
    if key:
        return key
    for env_file in ENV_FILES:
        if not env_file.exists():
            continue
        for line in env_file.read_text().splitlines():
            if line.startswith("TOGETHER_API_KEY="):
                key = line.split("=", 1)[1].strip().strip('"').strip("'")
                os.environ["TOGETHER_API_KEY"] = key
                return key
    print("ERROR: TOGETHER_API_KEY not found in env or " + " / ".join(str(p) for p in ENV_FILES))
    sys.exit(1)


def validate_dataset(data_path):
    """Walk every line of the dataset and confirm it parses + has the right shape.
    Returns (n_examples, n_tokens_estimate, errors)."""
    errors = []
    n = 0
    char_total = 0

    for i, raw in enumerate(open(data_path), 1):
        raw = raw.strip()
        if not raw:
            continue
        try:
            rec = json.loads(raw)
        except Exception as e:
            errors.append(f"line {i}: not JSON: {e}")
            continue
        n += 1
        msgs = rec.get("messages")
        if not isinstance(msgs, list) or len(msgs) < 3:
            errors.append(f"line {i}: needs ≥3 messages")
            continue
        roles = [m.get("role") for m in msgs]
        if roles[0] != "system":
            errors.append(f"line {i}: first message must be system, got {roles[0]}")
        # Check for consecutive assistant messages (tokenizer-breaking)
        for j in range(len(roles) - 1):
            if roles[j] == "assistant" and roles[j + 1] == "assistant":
                errors.append(f"line {i}: consecutive assistant messages at index {j}")
                break
        # Check tool calls have parseable arguments
        for m in msgs:
            for tc in m.get("tool_calls") or []:
                args = tc.get("function", {}).get("arguments", "")
                if isinstance(args, str):
                    try:
                        json.loads(args)
                    except Exception:
                        errors.append(f"line {i}: unparseable tool call args")
                        break
            # Tool messages need a tool_call_id
            if m.get("role") == "tool" and not m.get("tool_call_id"):
                errors.append(f"line {i}: tool message missing tool_call_id")
                break

        # Check tools array exists
        tools = rec.get("tools")
        if not isinstance(tools, list) or not tools:
            errors.append(f"line {i}: missing/empty top-level tools array")
        else:
            for t in tools:
                if t.get("type") != "function" or "function" not in t:
                    errors.append(f"line {i}: tool entry not OpenAI function shape")
                    break

        # Rough token estimate: ~4 chars/token for English/code mix
        char_total += len(raw)

    token_estimate = char_total // 4
    return n, token_estimate, errors


def estimate_cost(token_estimate, n_epochs, dollars_per_million_tokens=6.0):
    """Together AI LoRA on 30B-class models is roughly $5-8 / M training tokens."""
    total_tokens = token_estimate * n_epochs
    return total_tokens, total_tokens / 1_000_000 * dollars_per_million_tokens


def print_brief(args, n_examples, token_estimate, total_tokens, est_cost_low, est_cost_high):
    print()
    print("=" * 64)
    print("  TRAINING BRIEF — VALIDATION RUN (Phase 4 / G3 gate)")
    print("=" * 64)
    print()
    print(f"  Base model:           {BASE_MODEL}")
    print(f"  Method:               LoRA")
    print(f"  LoRA rank / alpha:    {LORA_R} / {LORA_ALPHA}")
    print(f"  LoRA dropout:         {LORA_DROPOUT}")
    print(f"  Trainable modules:    {LORA_TRAINABLE_MODULES}")
    print(f"                        ↑ EXCLUDES MoE router (Together default `all-linear` may not)")
    print(f"  Sequence length:      {SEQ_LENGTH}")
    print(f"  Batch size:           {BATCH_SIZE}")
    print(f"  Learning rate:        {LEARNING_RATE}")
    print(f"  Warmup ratio:         {WARMUP_RATIO}")
    print(f"  Epochs:               {N_EPOCHS}")
    print(f"  Suffix:               {SUFFIX}")
    print()
    print(f"  Dataset:              {args.data}")
    print(f"  Examples (lines):     {n_examples}")
    print(f"  Tokens (est):         {token_estimate:,}")
    print(f"  Train tokens × {N_EPOCHS}:    {total_tokens:,}")
    print()
    print(f"  Estimated cost:       ${est_cost_low:.2f} – ${est_cost_high:.2f}")
    print(f"                        (Together LoRA on 30B-class ≈ $5-8 / M tokens)")
    print(f"  Est. wall clock:      30-90 min")
    print()
    print(f"  Stage:                Phase 4 — VALIDATION RUN (small, cheap)")
    print(f"  Purpose:              Confirm pipeline + dataset format work end-to-end")
    print(f"                        before committing to the production run (Phase 7)")
    print()
    print("=" * 64)


def submit_job(args, train_file_id):
    from together import Together
    client = Together(api_key=load_api_key())

    job_params = {
        "training_file": train_file_id,
        "model": BASE_MODEL,
        "n_epochs": N_EPOCHS,
        "learning_rate": LEARNING_RATE,
        "batch_size": BATCH_SIZE,
        "warmup_ratio": WARMUP_RATIO,
        "suffix": SUFFIX,
        "lora": True,
        "lora_r": LORA_R,
        "lora_alpha": LORA_ALPHA,
        "lora_dropout": LORA_DROPOUT,
        "lora_trainable_modules": LORA_TRAINABLE_MODULES,
    }

    print()
    print("Submitting fine-tuning job...")
    response = client.fine_tuning.create(**job_params)
    job_id = response.id
    print(f"  job_id:  {job_id}")
    print(f"  status:  {response.status}")
    print()

    job_info = {
        "phase": "phase4_validation",
        "job_id": job_id,
        "model": BASE_MODEL,
        "train_file": train_file_id,
        "params": {k: v for k, v in job_params.items() if k != "training_file"},
        "submitted_at": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    info_path = SCRIPT_DIR / "phase4_job.json"
    info_path.write_text(json.dumps(job_info, indent=2))
    print(f"  Job info saved to {info_path}")
    print()
    print(f"Monitor: python3 ../train.py --status {job_id}")


def sanitize_for_upload(data_path):
    """Together's validator rejects unknown top-level fields. Strip our internal
    `_task_*` / `_wall_seconds` metadata into a sibling temp file used only for
    upload. The original file keeps the bookkeeping fields for our own use."""
    import tempfile
    keep_fields = {"messages", "tools"}
    tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False)
    n = 0
    for line in open(data_path):
        line = line.strip()
        if not line:
            continue
        rec = json.loads(line)
        clean = {k: v for k, v in rec.items() if k in keep_fields}
        tmp.write(json.dumps(clean) + "\n")
        n += 1
    tmp.close()
    print(f"  sanitized {n} examples to {tmp.name} (stripped _task_*/_wall_seconds metadata)")
    return Path(tmp.name)


def upload_dataset(data_path):
    from together import Together
    client = Together(api_key=load_api_key())
    sanitized = sanitize_for_upload(data_path)
    print(f"Uploading {sanitized}...")
    resp = client.files.upload(file=str(sanitized), purpose="fine-tune")
    print(f"  uploaded as {resp.id}")
    return resp.id


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--data", required=True, help="Path to validation_train.jsonl")
    p.add_argument("--confirm", action="store_true",
                   help="Actually submit the job (default is dry-run / brief only)")
    args = p.parse_args()

    data_path = Path(args.data)
    if not data_path.exists():
        sys.exit(f"ERROR: dataset file not found: {data_path}")

    print()
    print(f"Validating dataset: {data_path}")
    n, token_est, errors = validate_dataset(data_path)
    print(f"  examples:        {n}")
    print(f"  estimated tokens: {token_est:,}")
    print(f"  errors:           {len(errors)}")
    if errors:
        print()
        print("  --- first 10 errors ---")
        for e in errors[:10]:
            print(f"    {e}")
        sys.exit("  Refusing to proceed with dataset errors.")

    total_tokens, _ = estimate_cost(token_est, N_EPOCHS)
    _, est_low = estimate_cost(token_est, N_EPOCHS, dollars_per_million_tokens=5.0)
    _, est_high = estimate_cost(token_est, N_EPOCHS, dollars_per_million_tokens=8.0)
    print_brief(args, n, token_est, total_tokens, est_low, est_high)

    if not args.confirm:
        print()
        print("DRY RUN — no job submitted.")
        print("To actually submit, re-run with --confirm AFTER explicit user approval.")
        return

    # Final pre-flight: dataset already validated, brief printed
    print()
    print(">>> --confirm passed; submitting job <<<")
    train_file_id = upload_dataset(data_path)
    submit_job(args, train_file_id)


if __name__ == "__main__":
    main()
