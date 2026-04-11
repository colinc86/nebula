#!/usr/bin/env python3
"""HumanEval translator coverage harness.

For each of the 164 HumanEval problems, run:
  1. py → nebula  (`nebula from-py`)
  2. nebula → py  (`nebula to-py`)
  3. exec the round-tripped Python and run the problem's unit test

Reports:
  - parse-fail count (Step 1 produced empty/error)
  - roundtrip-fail count (Step 2 syntactically broken Python)
  - test-fail count (Step 3 wrong answer)
  - pass count

Target metric: ≥95% (≥156 of 164) pass through to test success.
"""

from __future__ import annotations
import json
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path
from typing import NamedTuple

REPO_ROOT = Path(__file__).resolve().parents[2]
NEBULA = REPO_ROOT / "compiler" / "target" / "debug" / "nebula"
HUMANEVAL = REPO_ROOT / "bench" / "corpus" / "humaneval_py" / "humaneval.jsonl"


class Result(NamedTuple):
    task_id: str
    parsed: bool
    roundtripped: bool
    tests_passed: bool
    error: str = ""


def run_nebula(args: list[str], input_text: str | None = None, timeout: float = 30.0) -> tuple[int, str, str]:
    proc = subprocess.run(
        [str(NEBULA), *args],
        input=input_text,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    return proc.returncode, proc.stdout, proc.stderr


def from_py(py_src: str) -> tuple[str, str]:
    with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False) as f:
        f.write(py_src)
        path = f.name
    try:
        rc, out, err = run_nebula(["from-py", path])
    finally:
        Path(path).unlink(missing_ok=True)
    return out, err


def to_py(neb_src: str) -> tuple[str, str]:
    with tempfile.NamedTemporaryFile("w", suffix=".neb", delete=False) as f:
        f.write(neb_src)
        path = f.name
    try:
        rc, out, err = run_nebula(["to-py", path])
    finally:
        Path(path).unlink(missing_ok=True)
    return out, err


def evaluate_problem(problem: dict) -> Result:
    task_id = problem["task_id"]
    full_python = problem["prompt"] + problem["canonical_solution"]

    # Step 1: py → nebula
    neb, err = from_py(full_python)
    if not neb.strip():
        return Result(task_id, False, False, False, f"from-py empty: {err[:200]}")

    # Step 2: nebula → py
    rt_py, err = to_py(neb)
    if not rt_py.strip():
        return Result(task_id, True, False, False, f"to-py empty: {err[:200]}")

    # Step 3: run unit tests
    test_program = (
        rt_py
        + "\n\n"
        + problem["test"]
        + f"\n\ncheck({problem['entry_point']})\n"
    )
    try:
        proc = subprocess.run(
            [sys.executable, "-c", test_program],
            capture_output=True,
            text=True,
            timeout=30.0,
        )
    except subprocess.TimeoutExpired:
        return Result(task_id, True, True, False, "test timeout")

    if proc.returncode == 0:
        return Result(task_id, True, True, True)
    return Result(task_id, True, True, False, (proc.stderr or proc.stdout)[:300])


def main() -> int:
    if not NEBULA.exists():
        print(f"nebula binary not found at {NEBULA}; run `cargo build` first", file=sys.stderr)
        return 2
    if not HUMANEVAL.exists():
        print(f"HumanEval not found at {HUMANEVAL}", file=sys.stderr)
        return 2

    problems = [json.loads(line) for line in HUMANEVAL.open()]
    print(f"Loaded {len(problems)} HumanEval problems")

    results: list[Result] = []
    for i, problem in enumerate(problems, 1):
        r = evaluate_problem(problem)
        results.append(r)
        status = "PASS" if r.tests_passed else (
            "rt_fail" if not r.roundtripped else (
                "test_fail" if r.parsed else "parse_fail"
            )
        )
        print(f"  [{i:3d}/{len(problems)}] {r.task_id:18s} {status}")

    parsed = sum(1 for r in results if r.parsed)
    roundtripped = sum(1 for r in results if r.roundtripped)
    passed = sum(1 for r in results if r.tests_passed)
    total = len(results)

    print()
    print("=" * 60)
    print(f"Parsed (from-py produced output): {parsed}/{total} ({parsed/total*100:.1f}%)")
    print(f"Roundtripped (to-py succeeded):   {roundtripped}/{total} ({roundtripped/total*100:.1f}%)")
    print(f"Tests passed:                     {passed}/{total} ({passed/total*100:.1f}%)")
    print("=" * 60)

    # Dump failures for inspection
    failures = [r for r in results if not r.tests_passed]
    if failures:
        log = REPO_ROOT / "bench" / "results" / "translator_coverage_failures.txt"
        log.parent.mkdir(parents=True, exist_ok=True)
        with log.open("w") as f:
            for r in failures:
                f.write(f"{r.task_id}\tparsed={r.parsed}\trt={r.roundtripped}\ttests={r.tests_passed}\n")
                if r.error:
                    f.write(textwrap.indent(r.error, "    ") + "\n")
        print(f"Failure log: {log}")

    return 0 if passed >= int(total * 0.95) else 1


if __name__ == "__main__":
    sys.exit(main())
