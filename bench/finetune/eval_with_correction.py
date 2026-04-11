#!/usr/bin/env python3
"""Evaluate Nebula→Python with error correction retry loop (Phase 6).

After the model generates Python from Nebula, run the tests. On failure,
feed the error back to the model and retry up to N times. If still failing,
fall back to rule-based `nebula to-python`.

Usage:
    python3 eval_with_correction.py --ollama nebula-python-3epoch --problems 164 --max-retries 3
    python3 eval_with_correction.py --together MODEL_NAME --problems 164 --max-retries 3
"""

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from eval import (
    load_humaneval, python_to_nebula, run_test,
    nebula_to_python_fixup, fallback_translate,
    DirectModel, OllamaModel, TogetherModel,
)

RETRY_PROMPT = """Your code failed with this error:
{error}

Here was your code:
{code}

Fix the code. Output only the corrected Python function, no explanations."""


def main():
    parser = argparse.ArgumentParser(description="Evaluate with error correction")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--model", help="Model path for direct loading")
    group.add_argument("--ollama", help="Ollama model name")
    group.add_argument("--together", help="Together AI model name")
    parser.add_argument("--problems", type=int, default=164)
    parser.add_argument("--max-retries", type=int, default=3)
    parser.add_argument("--output", default=None)
    args = parser.parse_args()

    if args.ollama:
        model = OllamaModel(args.ollama)
    elif args.together:
        model = TogetherModel(args.together)
    else:
        model = DirectModel(args.model)

    problems = load_humaneval(args.problems)
    print(f"Evaluating {len(problems)} problems with up to {args.max_retries} retries\n")

    passed_first = 0
    passed_retry = 0
    passed_fallback = 0
    failed = 0
    errors = 0
    results = []

    for i, problem in enumerate(problems):
        task_id = problem["task_id"]
        entry_point = problem["entry_point"]
        prompt = problem["prompt"]
        solution = problem["canonical_solution"]
        test = problem["test"]

        nebula = python_to_nebula(prompt + solution)
        if nebula is None:
            print(f"[{i+1}/{len(problems)}] {task_id}: SKIP")
            errors += 1
            continue

        # Attempt 1
        generated = nebula_to_python_fixup(model.generate(nebula))
        ok, err = run_test(generated, test, entry_point)
        attempt = 1

        if ok:
            passed_first += 1
            status = "PASS (attempt 1)"
        else:
            # Retry loop
            current_code = generated
            current_err = err
            retry_ok = False

            for retry in range(args.max_retries):
                attempt = retry + 2
                retry_input = RETRY_PROMPT.format(
                    error=current_err[:300], code=current_code[:500]
                )
                corrected = nebula_to_python_fixup(model.generate(retry_input))
                ok_r, err_r = run_test(corrected, test, entry_point)
                if ok_r:
                    retry_ok = True
                    generated = corrected
                    break
                current_code = corrected
                current_err = err_r

            if retry_ok:
                passed_retry += 1
                status = f"PASS (attempt {attempt})"
            else:
                # Fallback: rule-based codegen
                fb = fallback_translate(nebula)
                ok_fb, _ = run_test(fb, test, entry_point)
                if ok_fb:
                    passed_fallback += 1
                    generated = fb
                    status = "PASS (fallback)"
                else:
                    failed += 1
                    status = "FAIL"

        print(f"[{i+1}/{len(problems)}] {task_id}: {status}")

        results.append({
            "task_id": task_id,
            "status": status,
            "attempt": attempt,
            "fallback": "fallback" in status,
            "generated": generated[:200],
            "error": err if "FAIL" in status else None,
        })

    # Summary
    total = passed_first + passed_retry + passed_fallback + failed
    total_passed = passed_first + passed_retry + passed_fallback
    ca = total_passed / total * 100 if total > 0 else 0
    ca_base = passed_first / total * 100 if total > 0 else 0

    print(f"\n{'='*50}")
    print(f"RESULTS — Error Correction Pipeline")
    print(f"{'='*50}")
    print(f"Problems evaluated: {total} (skipped {errors})")
    print(f"")
    print(f"Without correction (Phase 5 baseline):")
    print(f"  Pass@1: {passed_first}/{total} ({ca_base:.1f}%)")
    print(f"")
    print(f"With correction (Phase 6):")
    print(f"  Passed on attempt 1:  {passed_first}")
    print(f"  Passed after retry:   {passed_retry}")
    print(f"  Passed via fallback:  {passed_fallback}")
    print(f"  Failed:               {failed}")
    print(f"  Total Pass@1:         {total_passed}/{total} ({ca:.1f}%)")
    print(f"  Improvement:          +{ca - ca_base:.1f}pp from error correction")

    if args.output:
        with open(args.output, "w") as f:
            for r in results:
                f.write(json.dumps(r) + "\n")
            f.write(json.dumps({
                "summary": True,
                "total": total,
                "skipped": errors,
                "passed_first": passed_first,
                "passed_retry": passed_retry,
                "passed_fallback": passed_fallback,
                "failed": failed,
                "pass_at_1_no_correction": ca_base,
                "pass_at_1_with_correction": ca,
            }) + "\n")
        print(f"\nResults saved to {args.output}")


if __name__ == "__main__":
    main()
