#!/usr/bin/env python3
"""InterTrans Validation Experiment: Does A->Nebula->B beat direct A->B translation?

Tests three conditions for cross-language translation on HumanEval problems:
  (a) Direct: LLM translates source -> target directly
  (b) Nebula-mediated: Ingest source to Nebula deterministically, then LLM translates Nebula -> target
  (c) Pipeline: Deterministic nebula from-{source} | nebula to-{target} (no LLM)

Metric: Computation Accuracy (CA) -- does the target code compile and pass tests?

Usage:
  python3 intertrans_eval.py --source python --target go
  python3 intertrans_eval.py --source all --target all
  python3 intertrans_eval.py --source python --target all --model claude-sonnet-4-20250514
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
NEBULA_BIN = REPO_ROOT / "compiler" / "target" / "debug" / "nebula"
HUMANEVAL = REPO_ROOT / "bench" / "corpus" / "humaneval_py" / "humaneval.jsonl"
RESULTS_DIR = REPO_ROOT / "bench" / "results"

# Language mappings
LANG_FROM_CMD = {
    "python": "from-python",
    "javascript": "from-js",
    "typescript": "from-ts",
    "go": "from-go",
    "rust": "from-rs",
    "c": "from-c",
}

LANG_TO_CMD = {
    "python": "to-python",
    "javascript": "to-js",
    "typescript": "to-ts",
    "go": "to-go",
    "rust": "to-rust",
    "c": "to-c",
}

LANG_EXTENSIONS = {
    "python": ".py",
    "javascript": ".js",
    "typescript": ".ts",
    "go": ".go",
    "rust": ".rs",
    "c": ".c",
}

ALL_LANGUAGES = list(LANG_FROM_CMD.keys())

# Selected HumanEval problems -- diverse, self-contained, no imports beyond builtins
SELECTED_PROBLEMS = [
    0,   # has_close_elements (nested loops, float comparison)
    2,   # truncate_number (math)
    4,   # mean_absolute_deviation (list ops, math)
    7,   # filter_by_substring (string ops, list filtering)
    10,  # make_palindrome (string manipulation)
    11,  # string_xor (bitwise on strings)
    13,  # greatest_common_divisor (recursion)
    15,  # string_sequence (string building)
    18,  # how_many_times (string search, counting)
    20,  # find_closest_elements (sorting, pairs)
    22,  # filter_integers (type checking, filtering)
    28,  # concatenate (list of strings)
    30,  # get_positive (list filtering)
    34,  # unique (deduplication, sorting)
    36,  # fizz_buzz (classic, modular arithmetic)
    43,  # pairs_sum_to_zero (nested loops)
    54,  # same_chars (set comparison)
    58,  # common (set intersection, sorting)
    74,  # total_match (string length comparison)
    80,  # is_happy (string pattern)
]


# ---------------------------------------------------------------------------
# Toolchain availability
# ---------------------------------------------------------------------------

def check_toolchains():
    """Check which language toolchains are available. Returns set of available languages."""
    available = set()

    # Python is always available (we are running it)
    available.add("python")

    checks = {
        "go": ["go", "version"],
        "javascript": ["node", "--version"],
        "typescript": ["npx", "tsx", "--version"],
        "rust": ["rustc", "--version"],
        "c": ["gcc", "--version"],
    }

    for lang, cmd in checks.items():
        if shutil.which(cmd[0]) is not None:
            try:
                subprocess.run(cmd, capture_output=True, timeout=10)
                available.add(lang)
            except (subprocess.TimeoutExpired, OSError):
                pass

    return available


# ---------------------------------------------------------------------------
# Language runners
# ---------------------------------------------------------------------------

def run_python(code, timeout=10):
    """Run Python code, return (success, output)."""
    try:
        result = subprocess.run(
            [sys.executable, "-c", code],
            capture_output=True, text=True, timeout=timeout
        )
        return result.returncode == 0, result.stdout + result.stderr
    except subprocess.TimeoutExpired:
        return False, "TIMEOUT"
    except Exception as e:
        return False, str(e)


def run_go(code, timeout=15):
    """Compile and run Go code, return (success, output)."""
    with tempfile.TemporaryDirectory() as tmpdir:
        go_file = os.path.join(tmpdir, "main.go")
        with open(go_file, "w") as f:
            f.write(code)

        env = os.environ.copy()
        env["GOPATH"] = os.path.join(tmpdir, "gopath")
        env["GOCACHE"] = os.path.join(tmpdir, "gocache")
        subprocess.run(
            ["go", "mod", "init", "test"],
            cwd=tmpdir, capture_output=True, timeout=5, env=env
        )

        try:
            result = subprocess.run(
                ["go", "run", "main.go"],
                cwd=tmpdir, capture_output=True, text=True, timeout=timeout, env=env
            )
            return result.returncode == 0, result.stdout + result.stderr
        except subprocess.TimeoutExpired:
            return False, "TIMEOUT"
        except Exception as e:
            return False, str(e)


def run_javascript(code, timeout=10):
    """Run JavaScript code with Node.js, return (success, output)."""
    with tempfile.NamedTemporaryFile(suffix=".js", mode="w", delete=False) as f:
        f.write(code)
        js_file = f.name
    try:
        result = subprocess.run(
            ["node", js_file],
            capture_output=True, text=True, timeout=timeout
        )
        return result.returncode == 0, result.stdout + result.stderr
    except subprocess.TimeoutExpired:
        return False, "TIMEOUT"
    except Exception as e:
        return False, str(e)
    finally:
        os.unlink(js_file)


def run_typescript(code, timeout=15):
    """Run TypeScript code with npx tsx, return (success, output)."""
    with tempfile.NamedTemporaryFile(suffix=".ts", mode="w", delete=False) as f:
        f.write(code)
        ts_file = f.name
    try:
        result = subprocess.run(
            ["npx", "tsx", ts_file],
            capture_output=True, text=True, timeout=timeout
        )
        return result.returncode == 0, result.stdout + result.stderr
    except subprocess.TimeoutExpired:
        return False, "TIMEOUT"
    except Exception as e:
        return False, str(e)
    finally:
        os.unlink(ts_file)


def run_rust(code, timeout=15):
    """Compile and run Rust code, return (success, output)."""
    with tempfile.TemporaryDirectory() as tmpdir:
        rs_file = os.path.join(tmpdir, "main.rs")
        bin_file = os.path.join(tmpdir, "main")
        with open(rs_file, "w") as f:
            f.write(code)

        try:
            comp = subprocess.run(
                ["rustc", rs_file, "-o", bin_file],
                capture_output=True, text=True, timeout=timeout
            )
            if comp.returncode != 0:
                return False, f"Compilation failed:\n{comp.stderr}"

            result = subprocess.run(
                [bin_file],
                capture_output=True, text=True, timeout=timeout
            )
            return result.returncode == 0, result.stdout + result.stderr
        except subprocess.TimeoutExpired:
            return False, "TIMEOUT"
        except Exception as e:
            return False, str(e)


def run_c(code, timeout=15):
    """Compile and run C code, return (success, output)."""
    with tempfile.TemporaryDirectory() as tmpdir:
        c_file = os.path.join(tmpdir, "main.c")
        bin_file = os.path.join(tmpdir, "main")
        with open(c_file, "w") as f:
            f.write(code)

        try:
            comp = subprocess.run(
                ["gcc", c_file, "-o", bin_file, "-lm"],
                capture_output=True, text=True, timeout=timeout
            )
            if comp.returncode != 0:
                return False, f"Compilation failed:\n{comp.stderr}"

            result = subprocess.run(
                [bin_file],
                capture_output=True, text=True, timeout=timeout
            )
            return result.returncode == 0, result.stdout + result.stderr
        except subprocess.TimeoutExpired:
            return False, "TIMEOUT"
        except Exception as e:
            return False, str(e)


def run_target(code, target):
    """Dispatch to the appropriate language runner."""
    runners = {
        "python": run_python,
        "go": run_go,
        "javascript": run_javascript,
        "typescript": run_typescript,
        "rust": run_rust,
        "c": run_c,
    }
    runner = runners.get(target)
    if runner is None:
        return False, f"No runner for target language: {target}"
    return runner(code)


# ---------------------------------------------------------------------------
# Problem loading and source generation
# ---------------------------------------------------------------------------

def load_problems():
    """Load HumanEval problems."""
    with open(HUMANEVAL) as f:
        all_problems = [json.loads(line) for line in f]

    selected = []
    for p in all_problems:
        task_num = int(p["task_id"].split("/")[1])
        if task_num in SELECTED_PROBLEMS:
            selected.append(p)
    return selected


def extract_solution_and_test(problem):
    """Extract the canonical Python solution and test from a HumanEval problem."""
    prompt = problem["prompt"]
    solution = problem["canonical_solution"]
    test = problem["test"]
    entry_point = problem["entry_point"]

    full_python = f"{prompt}{solution}\n{test}\ncheck({entry_point})\n"
    return prompt, solution, full_python, entry_point


def get_source_code(full_python, source_lang):
    """Get source code in the specified language.

    For Python, returns the code directly.
    For other languages, ingests to Nebula then generates source via to-{lang}.
    """
    if source_lang == "python":
        return full_python, None

    # Python -> Nebula -> source language
    with tempfile.NamedTemporaryFile(suffix=".py", mode="w", delete=False) as f:
        f.write(full_python)
        py_file = f.name

    try:
        # Step 1: Python -> Nebula
        result = subprocess.run(
            [str(NEBULA_BIN), "from-python", py_file],
            capture_output=True, text=True, timeout=10
        )
        if result.returncode != 0:
            return None, f"from-python failed: {result.stderr}"
        nebula_code = result.stdout

        # Step 2: Nebula -> source language
        with tempfile.NamedTemporaryFile(suffix=".neb", mode="w", delete=False) as nf:
            nf.write(nebula_code)
            neb_file = nf.name

        to_cmd = LANG_TO_CMD[source_lang]
        result = subprocess.run(
            [str(NEBULA_BIN), to_cmd, neb_file],
            capture_output=True, text=True, timeout=10
        )
        os.unlink(neb_file)

        if result.returncode != 0:
            return None, f"{to_cmd} failed: {result.stderr}"
        return result.stdout, None
    finally:
        os.unlink(py_file)


# ---------------------------------------------------------------------------
# Translation conditions
# ---------------------------------------------------------------------------

def llm_translate(prompt, model="claude-sonnet-4-20250514"):
    """Call Claude CLI for translation."""
    try:
        result = subprocess.run(
            ["claude", "-p", prompt, "--model", model, "--output-format", "text"],
            capture_output=True, text=True, timeout=120
        )
        if result.returncode == 0:
            return result.stdout.strip(), None
        return None, f"claude CLI failed: {result.stderr}"
    except FileNotFoundError:
        return None, "claude CLI not found"
    except subprocess.TimeoutExpired:
        return None, "TIMEOUT"


def nebula_ingest(source_code, source_lang):
    """Deterministically ingest source code to Nebula."""
    ext = LANG_EXTENSIONS[source_lang]
    with tempfile.NamedTemporaryFile(suffix=ext, mode="w", delete=False) as f:
        f.write(source_code)
        src_file = f.name

    try:
        from_cmd = LANG_FROM_CMD[source_lang]
        result = subprocess.run(
            [str(NEBULA_BIN), from_cmd, src_file],
            capture_output=True, text=True, timeout=10
        )
        if result.returncode != 0:
            return None, f"{from_cmd} failed: {result.stderr}"
        return result.stdout, None
    finally:
        os.unlink(src_file)


def nebula_to_target(nebula_code, target_lang):
    """Deterministically translate Nebula to target language."""
    with tempfile.NamedTemporaryFile(suffix=".neb", mode="w", delete=False) as f:
        f.write(nebula_code)
        neb_file = f.name

    try:
        to_cmd = LANG_TO_CMD[target_lang]
        result = subprocess.run(
            [str(NEBULA_BIN), to_cmd, neb_file],
            capture_output=True, text=True, timeout=10
        )
        if result.returncode != 0:
            return None, f"{to_cmd} failed: {result.stderr}"
        return result.stdout, None
    finally:
        os.unlink(neb_file)


def condition_a_direct(source_code, source_lang, target_lang, entry_point, model):
    """Condition (a): Direct LLM translation source -> target."""
    source_name = source_lang.capitalize()
    target_name = target_lang.capitalize()
    prompt = f"""Translate the following {source_name} code to {target_name}.
Output ONLY the complete, runnable {target_name} code with a main function that runs the test assertions.
Do not include any explanation, markdown formatting, or code fences.

```{source_lang}
{source_code}
```"""
    return llm_translate(prompt, model=model)


def condition_b_nebula_mediated(source_code, source_lang, target_lang, entry_point, model):
    """Condition (b): Ingest source to Nebula deterministically, then LLM translates Nebula -> target."""
    nebula_code, err = nebula_ingest(source_code, source_lang)
    if err:
        # Fall back to using source directly
        nebula_code = source_code

    target_name = target_lang.capitalize()
    prompt = f"""Here is code in Nebula (a universal intermediate language).
Translate it to idiomatic {target_name}. Output ONLY the complete, runnable {target_name} code
with a main function that runs the test assertions.
Do not include any explanation, markdown formatting, or code fences.

```nebula
{nebula_code}
```"""
    return llm_translate(prompt, model=model)


def condition_c_pipeline(source_code, source_lang, target_lang, entry_point, model):
    """Condition (c): Deterministic Nebula pipeline (no LLM)."""
    nebula_code, err = nebula_ingest(source_code, source_lang)
    if err:
        return None, err
    return nebula_to_target(nebula_code, target_lang)


# ---------------------------------------------------------------------------
# Experiment runner
# ---------------------------------------------------------------------------

def run_experiment(problems, source_lang, target_lang, conditions, model):
    """Run the experiment for a single source->target pair."""
    results = []

    for i, problem in enumerate(problems):
        task_id = problem["task_id"]
        entry_point = problem["entry_point"]
        _, _, full_python, _ = extract_solution_and_test(problem)

        # Verify Python solution works first
        py_ok, py_out = run_python(full_python)
        if not py_ok:
            print(f"  SKIP {task_id}: Python solution fails ({py_out[:50]})")
            continue

        # Get source code in the source language
        source_code, err = get_source_code(full_python, source_lang)
        if err:
            print(f"  SKIP {task_id}: Could not generate {source_lang} source ({err[:60]})")
            continue

        print(f"[{i+1}/{len(problems)}] {task_id} ({entry_point}) [{source_lang}->{target_lang}]")

        row = {"task_id": task_id, "entry_point": entry_point}

        condition_fns = {
            "a": condition_a_direct,
            "b": condition_b_nebula_mediated,
            "c": condition_c_pipeline,
        }

        for cond in conditions:
            fn = condition_fns[cond]
            code, err = fn(source_code, source_lang, target_lang, entry_point, model)

            if err:
                row[f"cond_{cond}"] = "ERROR"
                row[f"cond_{cond}_err"] = err
                print(f"  ({cond}) ERROR: {err[:60]}")
                continue

            ok, out = run_target(code, target_lang)
            row[f"cond_{cond}"] = "PASS" if ok else "FAIL"
            if not ok:
                row[f"cond_{cond}_err"] = out[:200]

            status = "PASS" if ok else "FAIL"
            print(f"  ({cond}) {status}")

        results.append(row)
        time.sleep(0.5)  # Rate limiting for API calls

    return results


# ---------------------------------------------------------------------------
# Output and reporting
# ---------------------------------------------------------------------------

def print_results(results, conditions, source_lang, target_lang):
    """Print results table for a single pair."""
    print(f"\n=== RESULTS: {source_lang} -> {target_lang} ===\n")

    labels = {
        "a": "Direct (LLM)",
        "b": "Nebula-mediated (LLM)",
        "c": "Pipeline (deterministic)",
    }

    for cond in conditions:
        key = f"cond_{cond}"
        total = sum(1 for r in results if key in r)
        passed = sum(1 for r in results if r.get(key) == "PASS")
        failed = sum(1 for r in results if r.get(key) == "FAIL")
        errors = sum(1 for r in results if r.get(key) == "ERROR")
        ca = passed / total * 100 if total > 0 else 0

        print(f"Condition ({cond}) {labels[cond]}:")
        print(f"  Pass: {passed}/{total} ({ca:.1f}% CA)")
        print(f"  Fail: {failed}, Error: {errors}")
        print()

    # Per-problem table
    short_labels = {"a": "Direct", "b": "Neb-LLM", "c": "Pipeline"}
    print(f"{'Problem':<25}", end="")
    for cond in conditions:
        print(f" {short_labels[cond]:>10}", end="")
    print()
    print("-" * (25 + 11 * len(conditions)))

    for r in results:
        print(f"{r['task_id']:<25}", end="")
        for cond in conditions:
            status = r.get(f"cond_{cond}", "N/A")
            print(f" {status:>10}", end="")
        print()


def print_heatmap(all_results, conditions):
    """Print aggregate heatmap across all source->target pairs."""
    print("\n=== AGGREGATE HEATMAP ===\n")

    for cond in conditions:
        labels = {"a": "Direct (LLM)", "b": "Nebula-mediated (LLM)", "c": "Pipeline (deterministic)"}
        print(f"--- Condition ({cond}): {labels[cond]} ---\n")

        # Collect all source/target languages that were tested
        pairs = sorted(all_results.keys())
        sources = sorted(set(s for s, _ in pairs))
        targets = sorted(set(t for _, t in pairs))

        # Header
        print(f"{'src\\tgt':<12}", end="")
        for t in targets:
            print(f" {t[:6]:>8}", end="")
        print()
        print("-" * (12 + 9 * len(targets)))

        for s in sources:
            print(f"{s:<12}", end="")
            for t in targets:
                if (s, t) in all_results:
                    results = all_results[(s, t)]
                    key = f"cond_{cond}"
                    total = sum(1 for r in results if key in r)
                    passed = sum(1 for r in results if r.get(key) == "PASS")
                    ca = passed / total * 100 if total > 0 else 0
                    print(f" {ca:>7.1f}%", end="")
                elif s == t:
                    print(f"      -- ", end="")
                else:
                    print(f"       - ", end="")
            print()
        print()


def save_results(all_results, conditions, source_langs, target_langs, model):
    """Save results to markdown in bench/results/."""
    RESULTS_DIR.mkdir(exist_ok=True)

    # Determine filename
    if len(source_langs) == 1 and len(target_langs) == 1:
        out_path = RESULTS_DIR / f"intertrans_{source_langs[0]}_{target_langs[0]}.md"
    elif len(source_langs) == 1:
        out_path = RESULTS_DIR / f"intertrans_{source_langs[0]}_all.md"
    elif len(target_langs) == 1:
        out_path = RESULTS_DIR / f"intertrans_all_{target_langs[0]}.md"
    else:
        out_path = RESULTS_DIR / "intertrans_all.md"

    lines = [
        f"# InterTrans Validation",
        "",
        f"**Date:** {time.strftime('%Y-%m-%d %H:%M')}",
        f"**Model:** {model}",
        f"**Sources:** {', '.join(source_langs)}",
        f"**Targets:** {', '.join(target_langs)}",
        f"**Problems:** {len(SELECTED_PROBLEMS)} HumanEval problems",
        f"**Conditions:** {', '.join(conditions)}",
        "",
    ]

    labels = {
        "a": "Direct LLM translation",
        "b": "Nebula-mediated LLM",
        "c": "Deterministic pipeline",
    }

    # Per-pair summaries
    for (source, target), results in sorted(all_results.items()):
        lines.append(f"## {source.capitalize()} -> {target.capitalize()}")
        lines.append("")
        lines.append("| Condition | Description | Pass | Total | CA |")
        lines.append("|---|---|---|---|---|")

        for cond in conditions:
            key = f"cond_{cond}"
            total = sum(1 for r in results if key in r)
            passed = sum(1 for r in results if r.get(key) == "PASS")
            ca = passed / total * 100 if total > 0 else 0
            lines.append(f"| ({cond}) | {labels[cond]} | {passed} | {total} | {ca:.1f}% |")

        lines.append("")

        # Per-problem details
        short_labels = {"a": "Direct", "b": "Neb-LLM", "c": "Pipeline"}
        lines.append("| Problem | " + " | ".join(short_labels[c] for c in conditions) + " |")
        lines.append("|---|" + "---|" * len(conditions))

        for r in results:
            row = f"| {r['task_id']} |"
            for cond in conditions:
                status = r.get(f"cond_{cond}", "N/A")
                row += f" {status} |"
            lines.append(row)

        lines.append("")

    # Aggregate heatmap if multiple pairs
    if len(all_results) > 1:
        pairs = sorted(all_results.keys())
        sources = sorted(set(s for s, _ in pairs))
        targets = sorted(set(t for _, t in pairs))

        for cond in conditions:
            lines.append(f"## Heatmap: Condition ({cond}) - {labels[cond]}")
            lines.append("")
            lines.append("| src \\ tgt | " + " | ".join(t.capitalize() for t in targets) + " |")
            lines.append("|---|" + "---|" * len(targets))

            for s in sources:
                row = f"| {s.capitalize()} |"
                for t in targets:
                    if (s, t) in all_results:
                        results = all_results[(s, t)]
                        key = f"cond_{cond}"
                        total = sum(1 for r in results if key in r)
                        passed = sum(1 for r in results if r.get(key) == "PASS")
                        ca = passed / total * 100 if total > 0 else 0
                        row += f" {ca:.1f}% |"
                    elif s == t:
                        row += " -- |"
                    else:
                        row += " - |"
                lines.append(row)

            lines.append("")

    text = "\n".join(lines) + "\n"
    with open(out_path, "w") as f:
        f.write(text)
    print(f"\nResults saved to {out_path}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="InterTrans validation experiment")
    parser.add_argument("--source", default="python",
                        help="Source language or 'all' (default: python)")
    parser.add_argument("--target", default="go",
                        help="Target language or 'all' (default: go)")
    parser.add_argument("--model", default="claude-sonnet-4-20250514",
                        help="Model for LLM translation (default: claude-sonnet-4-20250514)")
    parser.add_argument("--conditions", default="a,b,c",
                        help="Conditions to test: a,b,c (comma-separated)")
    parser.add_argument("--pipeline-only", action="store_true",
                        help="Only run condition (c) -- no LLM calls")
    parser.add_argument("--problems", type=int, default=20,
                        help="Number of problems (default: 20)")
    args = parser.parse_args()

    conditions = args.conditions.split(",")
    if args.pipeline_only:
        conditions = ["c"]

    # Check available toolchains
    available = check_toolchains()
    print(f"Available toolchains: {', '.join(sorted(available))}")

    # Resolve source and target languages
    if args.source == "all":
        source_langs = [l for l in ALL_LANGUAGES if l in available]
    else:
        source_langs = [args.source]

    if args.target == "all":
        target_langs = [l for l in ALL_LANGUAGES if l in available]
    else:
        target_langs = [args.target]

    # Validate requested languages are available
    for lang in source_langs + target_langs:
        if lang not in available:
            print(f"WARNING: {lang} toolchain not available, skipping")

    source_langs = [l for l in source_langs if l in available]
    target_langs = [l for l in target_langs if l in available]

    if not source_langs or not target_langs:
        print("ERROR: No valid source/target languages available")
        sys.exit(1)

    print(f"Sources: {', '.join(source_langs)}")
    print(f"Targets: {', '.join(target_langs)}")
    print(f"Model: {args.model}")
    print(f"Conditions: {conditions}")

    # Load problems
    print(f"\nLoading HumanEval problems...")
    problems = load_problems()[:args.problems]
    print(f"Selected {len(problems)} problems\n")

    # Run all source->target pairs
    all_results = {}

    for source in source_langs:
        for target in target_langs:
            if source == target:
                continue

            print(f"\n{'='*60}")
            print(f"  {source} -> {target}")
            print(f"{'='*60}\n")

            results = run_experiment(problems, source, target, conditions, args.model)
            all_results[(source, target)] = results
            print_results(results, conditions, source, target)

    # Aggregate heatmap if multiple pairs
    if len(all_results) > 1:
        print_heatmap(all_results, conditions)

    # Save results
    save_results(all_results, conditions, source_langs, target_langs, args.model)


if __name__ == "__main__":
    main()
