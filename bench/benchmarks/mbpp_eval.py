#!/usr/bin/env python3
"""Evaluate a Nebula-Python model on MBPP (Mostly Basic Programming Problems).

Takes MBPP problems, converts the Python solution to Nebula, feeds it to the
model, and checks if the generated Python output passes the test assertions.

Requires: pip install datasets

Usage:
    # Evaluate via Ollama:
    python3 mbpp_eval.py --ollama nebula-python-3epoch --problems 50

    # Full 500-problem test split:
    python3 mbpp_eval.py --ollama nebula-python-3epoch --problems 500

    # Save results:
    python3 mbpp_eval.py --ollama nebula-python-3epoch --problems 100 --output mbpp_results.jsonl
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

# Import shared utilities from bench/finetune/eval.py
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'finetune'))
from eval import clean_response, nebula_to_python_fixup, fallback_translate, OllamaModel, NEBULA_BIN

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_mbpp(max_problems=None):
    """Load MBPP test split from HuggingFace."""
    try:
        from datasets import load_dataset
    except ImportError:
        print("ERROR: 'datasets' package required. Install with: pip install datasets")
        sys.exit(1)

    # Try loading without auth first; fall back to HF_TOKEN if needed
    try:
        ds = load_dataset('google-research-datasets/mbpp', split='test')
    except Exception:
        hf_token = None
        env_file = REPO_ROOT / "openai.env"
        if env_file.exists():
            for line in env_file.read_text().splitlines():
                if line.startswith("HF_TOKEN="):
                    hf_token = line.split("=", 1)[1].strip().strip('"')
                    break
        if not hf_token:
            import os
            hf_token = os.environ.get("HF_TOKEN")
        if not hf_token:
            print("ERROR: Could not load MBPP and no HF_TOKEN found")
            sys.exit(1)
        ds = load_dataset('google-research-datasets/mbpp', split='test', token=hf_token)

    problems = list(ds)
    if max_problems:
        problems = problems[:max_problems]
    return problems


def python_to_nebula(python_code):
    """Convert Python to Nebula using the compiler."""
    import os
    import tempfile
    with tempfile.NamedTemporaryFile(suffix=".py", mode="w", delete=False) as f:
        f.write(python_code)
        py_file = f.name
    try:
        result = subprocess.run(
            [str(NEBULA_BIN), "from-python", py_file],
            capture_output=True, text=True, timeout=10,
        )
        return result.stdout.strip() if result.returncode == 0 else None
    except Exception:
        return None
    finally:
        os.unlink(py_file)


def run_test(solution_code, test_list, test_setup_code="", timeout=10):
    """Run the generated Python solution against MBPP test assertions."""
    parts = []
    if test_setup_code:
        parts.append(test_setup_code)
    parts.append(solution_code)
    parts.append("\n".join(test_list))
    full_code = "\n".join(parts)

    try:
        result = subprocess.run(
            [sys.executable, "-c", full_code],
            capture_output=True, text=True, timeout=timeout,
        )
        return result.returncode == 0, result.stderr[:200]
    except subprocess.TimeoutExpired:
        return False, "TIMEOUT"
    except Exception as e:
        return False, str(e)


def main():
    parser = argparse.ArgumentParser(description="Evaluate Nebula-Python model on MBPP")
    parser.add_argument("--ollama", required=True, help="Ollama model name")
    parser.add_argument("--problems", type=int, default=50, help="Number of MBPP problems (max 500)")
    parser.add_argument("--output", default=None, help="Output results JSONL path")
    args = parser.parse_args()

    model = OllamaModel(args.ollama)

    problems = load_mbpp(args.problems)
    print(f"Evaluating on {len(problems)} MBPP problems\n")

    passed = 0
    failed = 0
    errors = 0
    results = []

    for i, problem in enumerate(problems):
        task_id = problem["task_id"]
        description = problem["text"]
        solution = problem["code"]
        test_list = problem["test_list"]
        test_setup = problem.get("test_setup_code", "") or ""

        # Convert reference solution to Nebula
        nebula = python_to_nebula(solution)
        if nebula is None:
            print(f"[{i+1}/{len(problems)}] task_{task_id}: SKIP (nebula conversion failed)")
            errors += 1
            continue

        # Generate Python from Nebula via model
        generated = nebula_to_python_fixup(model.generate(nebula))

        # Test the generated code
        ok, err = run_test(generated, test_list, test_setup)

        status = "PASS" if ok else "FAIL"
        if ok:
            passed += 1
        else:
            failed += 1

        print(f"[{i+1}/{len(problems)}] task_{task_id}: {status}")

        results.append({
            "task_id": task_id,
            "description": description[:200],
            "status": status,
            "nebula_input": nebula[:200],
            "generated": generated[:200],
            "error": err if not ok else None,
        })

    # Summary
    total = passed + failed
    pass_rate = passed / total * 100 if total > 0 else 0
    print(f"\n=== MBPP RESULTS ===")
    print(f"Pass@1: {passed}/{total} ({pass_rate:.1f}%)")
    print(f"Errors (skip): {errors}")

    if args.output:
        with open(args.output, "w") as f:
            for r in results:
                f.write(json.dumps(r) + "\n")
        print(f"Results saved to {args.output}")


if __name__ == "__main__":
    main()
