#!/usr/bin/env python3
"""Prepare training data for Nebula 1.0 neural refinement.

Downloads Python functions from HuggingFace's StarCoderData (or The Stack),
runs them through `nebula from-python`, and produces SFT training pairs:
  {"messages": [{"role": "user", "content": "<nebula>"}, {"role": "assistant", "content": "<python>"}]}

The model learns: given Nebula IL, produce idiomatic Python.

Usage:
    # Step 1: Download Python functions
    python3 prepare_data.py download --output raw_python.jsonl --count 50000

    # Step 2: Generate (nebula, python) pairs
    python3 prepare_data.py generate --input raw_python.jsonl --output train.jsonl

    # Step 3: Split into train/val
    python3 prepare_data.py split --input train.jsonl --train train_final.jsonl --val val_final.jsonl

    # All-in-one:
    python3 prepare_data.py all --count 20000 --output-dir data/
"""

import argparse
import json
import os
import random
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from concurrent.futures import ProcessPoolExecutor, as_completed

REPO_ROOT = Path(__file__).resolve().parents[2]
NEBULA_BIN = REPO_ROOT / "compiler" / "target" / "debug" / "nebula"

# Minimum/maximum function sizes (in characters) to keep
MIN_FUNC_SIZE = 50
MAX_FUNC_SIZE = 2000
# Maximum token count for training examples
MAX_TOKENS = 512


def download_python_functions(output_path, count=20000):
    """Collect Python functions from available sources."""
    collected = 0

    with open(output_path, "w") as f:
        # Source 1: HumanEval problems (164 functions)
        humaneval_path = REPO_ROOT / "bench" / "corpus" / "humaneval_py" / "humaneval.jsonl"
        if humaneval_path.exists():
            print("  Source 1: HumanEval...")
            with open(humaneval_path) as hf:
                for line in hf:
                    p = json.loads(line)
                    func = p.get("prompt", "") + p.get("canonical_solution", "")
                    if MIN_FUNC_SIZE <= len(func) <= MAX_FUNC_SIZE:
                        f.write(json.dumps({"python": func}) + "\n")
                        collected += 1
            print(f"    {collected} functions from HumanEval")

        # Source 2: Python standard library source files
        print("  Source 2: Python stdlib...")
        stdlib_count = 0
        for stdlib_dir in sys.path:
            if not os.path.isdir(stdlib_dir):
                continue
            for root, dirs, files in os.walk(stdlib_dir):
                for fname in files:
                    if not fname.endswith(".py"):
                        continue
                    if collected >= count:
                        break
                    try:
                        fpath = os.path.join(root, fname)
                        with open(fpath, errors="ignore") as pf:
                            content = pf.read()
                        functions = extract_functions(content)
                        for func in functions:
                            if MIN_FUNC_SIZE <= len(func) <= MAX_FUNC_SIZE:
                                f.write(json.dumps({"python": func}) + "\n")
                                collected += 1
                                stdlib_count += 1
                                if collected >= count:
                                    break
                    except Exception:
                        continue
                if collected >= count:
                    break
            if collected >= count:
                break
        print(f"    {stdlib_count} functions from stdlib")

        # Source 3: Try HuggingFace datasets (if auth available)
        if collected < count:
            print(f"  Source 3: HuggingFace datasets (need {count - collected} more)...")
            try:
                from datasets import load_dataset
                ds = load_dataset(
                    "bigcode/starcoderdata",
                    data_dir="python",
                    split="train",
                    streaming=True,
                    token=os.environ.get("HF_TOKEN"),
                )
                hf_count = 0
                for sample in ds:
                    if collected >= count:
                        break
                    content = sample.get("content", "")
                    if not content or len(content) < MIN_FUNC_SIZE:
                        continue
                    functions = extract_functions(content)
                    for func in functions:
                        if MIN_FUNC_SIZE <= len(func) <= MAX_FUNC_SIZE:
                            f.write(json.dumps({"python": func}) + "\n")
                            collected += 1
                            hf_count += 1
                            if collected >= count:
                                break
                    if hf_count % 1000 == 0 and hf_count > 0:
                        print(f"    {hf_count} from HuggingFace ({collected} total)...")
                print(f"    {hf_count} functions from HuggingFace")
            except Exception as e:
                print(f"    HuggingFace unavailable: {e}")
                print(f"    Proceeding with {collected} functions")

    print(f"Total: {collected} functions → {output_path}")
    return collected


def extract_functions(source):
    """Extract top-level function definitions from Python source."""
    functions = []
    lines = source.split("\n")
    current_func = []
    in_func = False
    func_indent = 0

    for line in lines:
        stripped = line.lstrip()
        indent = len(line) - len(stripped)

        if stripped.startswith("def ") and (not in_func or indent <= func_indent):
            # Save previous function
            if current_func:
                func_text = "\n".join(current_func).rstrip()
                if func_text:
                    functions.append(func_text)

            current_func = [line]
            in_func = True
            func_indent = indent
        elif in_func:
            if stripped and indent <= func_indent and not stripped.startswith("def "):
                # End of function — different top-level construct
                func_text = "\n".join(current_func).rstrip()
                if func_text:
                    functions.append(func_text)
                current_func = []
                in_func = False
            else:
                current_func.append(line)

    # Last function
    if current_func:
        func_text = "\n".join(current_func).rstrip()
        if func_text:
            functions.append(func_text)

    return functions


def python_to_nebula(python_code):
    """Convert Python code to Nebula using the compiler."""
    with tempfile.NamedTemporaryFile(suffix=".py", mode="w", delete=False) as f:
        f.write(python_code)
        py_file = f.name

    try:
        result = subprocess.run(
            [str(NEBULA_BIN), "from-python", py_file],
            capture_output=True, text=True, timeout=10,
        )
        if result.returncode != 0:
            return None
        nebula = result.stdout.strip()
        if not nebula or len(nebula) < 10:
            return None
        return nebula
    except (subprocess.TimeoutExpired, Exception):
        return None
    finally:
        os.unlink(py_file)


def is_valid_pair(python_code, nebula_code):
    """Check if a (nebula, python) pair is valid for training."""
    # Skip if Nebula is too similar to Python (no real transformation)
    if nebula_code == python_code:
        return False

    # Skip if Nebula contains error markers
    if "// @fallback" in nebula_code or "ERROR" in nebula_code:
        return False

    # Skip if too short or too long
    if len(nebula_code) < 20 or len(nebula_code) > 3000:
        return False

    # Skip if it's mostly comments
    code_lines = [l for l in nebula_code.splitlines() if l.strip() and not l.strip().startswith("//")]
    if len(code_lines) < 2:
        return False

    return True


def generate_pairs(input_path, output_path, workers=4):
    """Generate (nebula, python) training pairs."""
    print(f"Generating training pairs from {input_path}...")

    # Load raw Python functions
    with open(input_path) as f:
        items = [json.loads(line) for line in f]

    print(f"  {len(items)} Python functions loaded")

    # Process in parallel
    valid = 0
    skipped = 0

    with open(output_path, "w") as out:
        # Process in batches to show progress
        batch_size = 100
        for batch_start in range(0, len(items), batch_size):
            batch = items[batch_start:batch_start + batch_size]

            for item in batch:
                python_code = item["python"]
                nebula_code = python_to_nebula(python_code)

                if nebula_code is None:
                    skipped += 1
                    continue

                if not is_valid_pair(python_code, nebula_code):
                    skipped += 1
                    continue

                # SFT format: user provides Nebula, assistant produces Python
                pair = {
                    "messages": [
                        {
                            "role": "system",
                            "content": "You are a code translator. Given code in Nebula (a universal intermediate language), produce the equivalent idiomatic Python code.",
                        },
                        {"role": "user", "content": nebula_code},
                        {"role": "assistant", "content": python_code},
                    ]
                }
                out.write(json.dumps(pair) + "\n")
                valid += 1

            processed = min(batch_start + batch_size, len(items))
            print(f"  Processed {processed}/{len(items)} | Valid: {valid} | Skipped: {skipped}")

    print(f"\nGenerated {valid} valid pairs (skipped {skipped}) → {output_path}")
    return valid


def split_data(input_path, train_path, val_path, val_ratio=0.1):
    """Split data into train and validation sets."""
    with open(input_path) as f:
        lines = f.readlines()

    random.shuffle(lines)
    val_count = max(1, int(len(lines) * val_ratio))
    val_lines = lines[:val_count]
    train_lines = lines[val_count:]

    with open(train_path, "w") as f:
        f.writelines(train_lines)
    with open(val_path, "w") as f:
        f.writelines(val_lines)

    print(f"Split: {len(train_lines)} train, {len(val_lines)} val")


def run_all(count, output_dir):
    """Run the full pipeline: download → generate → split."""
    os.makedirs(output_dir, exist_ok=True)
    raw_path = os.path.join(output_dir, "raw_python.jsonl")
    pairs_path = os.path.join(output_dir, "pairs.jsonl")
    train_path = os.path.join(output_dir, "train.jsonl")
    val_path = os.path.join(output_dir, "val.jsonl")

    # Step 1: Download
    if not os.path.exists(raw_path):
        download_python_functions(raw_path, count=count)
    else:
        print(f"Using existing {raw_path}")

    # Step 2: Generate pairs
    generate_pairs(raw_path, pairs_path)

    # Step 3: Split
    split_data(pairs_path, train_path, val_path)

    print(f"\nDone! Training data ready in {output_dir}/")
    print(f"  train.jsonl: {sum(1 for _ in open(train_path))} examples")
    print(f"  val.jsonl:   {sum(1 for _ in open(val_path))} examples")


def main():
    parser = argparse.ArgumentParser(description="Prepare Nebula training data")
    sub = parser.add_subparsers(dest="command")

    dl = sub.add_parser("download", help="Download Python functions")
    dl.add_argument("--output", required=True)
    dl.add_argument("--count", type=int, default=20000)

    gen = sub.add_parser("generate", help="Generate (nebula, python) pairs")
    gen.add_argument("--input", required=True)
    gen.add_argument("--output", required=True)
    gen.add_argument("--workers", type=int, default=4)

    sp = sub.add_parser("split", help="Split into train/val")
    sp.add_argument("--input", required=True)
    sp.add_argument("--train", required=True)
    sp.add_argument("--val", required=True)
    sp.add_argument("--val-ratio", type=float, default=0.1)

    al = sub.add_parser("all", help="Run full pipeline")
    al.add_argument("--count", type=int, default=20000)
    al.add_argument("--output-dir", default="data")

    args = parser.parse_args()

    if args.command == "download":
        download_python_functions(args.output, args.count)
    elif args.command == "generate":
        generate_pairs(args.input, args.output, args.workers)
    elif args.command == "split":
        split_data(args.input, args.train, args.val, args.val_ratio)
    elif args.command == "all":
        run_all(args.count, args.output_dir)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
