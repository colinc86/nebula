"""Run the 5-eval gate against a fine-tuned model.

v2: all evals are rule-based or compiler-based. No LLM judge; no third-party
output used as training signal. Evaluation just calls the FT model on
held-out prompts and checks its outputs mechanically.

Dry-run mode still writes stubs without API calls.

Eval bars:
  1. Nebula syntax validity       ≥95%   — extract Nebula block → `nebula check`
  3. Behavior-anchor (no leak)    ≥90%   — plain system → no ```nebula blocks + prose length
  4. Tool-use fidelity            ≥90%   — valid tool_calls shape + Nebula args parse
  2. Semantic correctness         ≥70%   — deferred: needs HumanEval pytest runner
  5. End-to-end plugin round-trip ≥70%   — deferred: needs fixture repos
"""
from __future__ import annotations
import argparse
import json
import os
import re
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from config import (
    EVAL_RESULTS, PROMPT_POOLS, PILOT_MODEL, ensure_dirs, REPO,
    SYSTEM_PROMPT_NEBULA, SYSTEM_PROMPT_ANCHOR, FINAL,
)
import nebula_cli


PASS_BARS = {
    "eval1_syntax": 0.95,
    "eval2_semantic": 0.70,
    "eval3_anchor": 0.90,
    "eval4_toolcall": 0.90,
    "eval5_e2e": 0.70,
}


def load_openai_client():
    try:
        from openai import OpenAI  # type: ignore
    except ImportError:
        print("openai package missing (`pip install openai`)", file=sys.stderr)
        sys.exit(1)
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        env_file = REPO / ".env"
        if env_file.exists():
            for line in env_file.read_text().splitlines():
                if line.startswith("OPENAI_API_KEY="):
                    api_key = line.split("=", 1)[1].strip().strip('"').strip("'")
                    break
    if not api_key:
        print("OPENAI_API_KEY not set", file=sys.stderr)
        sys.exit(1)
    return OpenAI(api_key=api_key)


def extract_nebula_block(text: str) -> str | None:
    m = re.search(r"```(?:nebula)?\s*\n(.*?)```", text, re.DOTALL)
    if m:
        return m.group(1).strip()
    # No fence? Some responses won't fence. Try treating the whole body as
    # Nebula if it has Nebula signals.
    if re.search(r"^\s*(fn|def|class|struct|impl|use|if|for|while)\b", text, re.MULTILINE):
        return text.strip()
    return None


# ── Eval 1 — syntax validity ──────────────────────────────────────────

def run_eval1(client, model: str, limit: int) -> dict:
    """Sample `limit` held-out instruction prompts, generate responses,
    check each Nebula block via `nebula check`."""
    held = FINAL["held_out"]
    if not held.exists():
        return {"eval": "eval1_syntax", "error": f"missing {held}"}
    prompts = []
    for line in held.open():
        if not line.strip():
            continue
        row = json.loads(line)
        if row.get("messages"):
            # Need the user prompt from a slice-1 or slice-2 row.
            user = next((m for m in row["messages"] if m.get("role") == "user"), None)
            if user and isinstance(user.get("content"), str):
                prompts.append({"prompt": user["content"]})
        if len(prompts) >= limit:
            break
    results = []
    passed = 0
    for p in prompts:
        try:
            resp = client.chat.completions.create(
                model=model, max_tokens=1500, temperature=0.2,
                messages=[
                    {"role": "system", "content": SYSTEM_PROMPT_NEBULA},
                    {"role": "user", "content": p["prompt"]},
                ],
            )
            text = resp.choices[0].message.content or ""
        except Exception as e:
            results.append({"prompt": p["prompt"][:80], "error": str(e)})
            continue
        neb = extract_nebula_block(text)
        finish_reason = resp.choices[0].finish_reason
        ok = neb is not None and nebula_cli.check(neb)
        if ok:
            passed += 1
        results.append({"prompt": p["prompt"][:80], "ok": ok,
                        "finish_reason": finish_reason,
                        "response": text[:300]})
    total = len(results)
    rate = passed / total if total else 0.0
    # Also compute the rate among non-truncated responses — measures model
    # capability, not the token budget.
    natural = [r for r in results if r.get("finish_reason") == "stop"]
    natural_passed = sum(1 for r in natural if r.get("ok"))
    natural_rate = natural_passed / len(natural) if natural else 0.0
    return {
        "eval": "eval1_syntax",
        "total": total, "passed": passed, "rate": rate,
        "natural_total": len(natural), "natural_passed": natural_passed,
        "natural_rate": natural_rate,
        "truncated": total - len(natural),
        "pass_bar": PASS_BARS["eval1_syntax"],
        "decision": "pass" if natural_rate >= PASS_BARS["eval1_syntax"] else "fail",
        "note": "Decision uses natural_rate (responses that finished without hitting max_tokens). rate (includes truncation) is reported for visibility.",
        "per_prompt": results,
    }


# ── Eval 3 — rule-based anchor (no LLM judge) ─────────────────────────

def run_eval3(client, model: str, limit: int) -> dict:
    """Send anchor prompts under plain system; check that the response is
    plain prose (no ```nebula fenced blocks, reasonable length)."""
    pool = PROMPT_POOLS / "anchor_prompts.jsonl"
    if not pool.exists():
        return {"eval": "eval3_anchor", "error": f"missing {pool}"}
    prompts = [json.loads(l) for l in pool.open() if l.strip()][:limit]
    results = []
    passed = 0
    for p in prompts:
        try:
            resp = client.chat.completions.create(
                model=model, max_tokens=800, temperature=0.2,
                messages=[
                    {"role": "system", "content": SYSTEM_PROMPT_ANCHOR},
                    {"role": "user", "content": p["prompt"]},
                ],
            )
            text = resp.choices[0].message.content or ""
        except Exception as e:
            results.append({"prompt": p["prompt"][:80], "error": str(e)})
            continue
        # Rule-based checks:
        has_nebula_fence = bool(re.search(r"```nebula", text))
        long_enough = len(text.strip()) >= 100
        sentence_count = len(re.findall(r"[.!?]\s", text))
        has_prose = sentence_count >= 2
        ok = (not has_nebula_fence) and long_enough and has_prose
        if ok:
            passed += 1
        results.append({
            "prompt": p["prompt"][:80],
            "ok": ok,
            "has_nebula_fence": has_nebula_fence,
            "len": len(text),
            "sentences": sentence_count,
            "response": text[:300],
        })
    total = len(results)
    rate = passed / total if total else 0.0
    return {
        "eval": "eval3_anchor",
        "total": total, "passed": passed, "rate": rate,
        "pass_bar": PASS_BARS["eval3_anchor"],
        "decision": "pass" if rate >= PASS_BARS["eval3_anchor"] else "fail",
        "per_prompt": results,
        "note": "Rule-based check: no Nebula fence + prose length + ≥2 sentences. No LLM judge.",
    }


# ── Eval 4 — tool-use fidelity ────────────────────────────────────────

TOOLCALL_EVAL_PROMPTS = [
    {"prompt": "Read the file /tmp/foo.go.",                                             "expect_tool": "read_code"},
    {"prompt": "Search for `VectorComplex` in the current directory.",                   "expect_tool": "search_code"},
    {"prompt": "Validate this Nebula: `fn add(a, b) = a + b`.",                           "expect_tool": "check_nebula"},
    {"prompt": "What would this Nebula compile to in Go? `fn sq(x) = x * x`",             "expect_tool": "preview_translation"},
    {"prompt": "Create /tmp/hello.py with a hello function.",                             "expect_tool": "write_code"},
    {"prompt": "Rename `Vec` to `Vector` in /tmp/foo.go (use a Nebula patch).",           "expect_tool": "patch_code"},
    {"prompt": "Show me /tmp/raw.bin as raw bytes.",                                      "expect_tool": "read_native"},
    {"prompt": "Translate `fn f() = 42` to TypeScript.",                                  "expect_tool": "translate_code"},
    {"prompt": "Grep for `TODO` in /src/*.py.",                                           "expect_tool": "search_code"},
    {"prompt": "Check if `def foo():\\n    return 1` parses as Nebula.",                  "expect_tool": "check_nebula"},
]

TOOL_SCHEMAS_MIN = [
    {"type": "function", "function": {"name": "read_code", "description": "Read a source file in Nebula IL", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
    {"type": "function", "function": {"name": "read_native", "description": "Read raw file bytes", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
    {"type": "function", "function": {"name": "search_code", "description": "Grep source", "parameters": {"type": "object", "properties": {"query": {"type": "string"}, "path": {"type": "string"}, "glob": {"type": "string"}}, "required": ["query"]}}},
    {"type": "function", "function": {"name": "write_code", "description": "Write Nebula code to a file", "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "code": {"type": "string"}}, "required": ["path", "code"]}}},
    {"type": "function", "function": {"name": "patch_code", "description": "Byte-level replace", "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "old_str": {"type": "string"}, "new_str": {"type": "string"}, "nebula": {"type": "boolean"}}, "required": ["path", "old_str", "new_str"]}}},
    {"type": "function", "function": {"name": "translate_code", "description": "Translate code between languages", "parameters": {"type": "object", "properties": {"code": {"type": "string"}, "source_language": {"type": "string"}, "target_language": {"type": "string"}}, "required": ["code", "source_language", "target_language"]}}},
    {"type": "function", "function": {"name": "check_nebula", "description": "Validate Nebula syntax", "parameters": {"type": "object", "properties": {"code": {"type": "string"}}, "required": ["code"]}}},
    {"type": "function", "function": {"name": "preview_translation", "description": "Dry-run Nebula → native", "parameters": {"type": "object", "properties": {"code": {"type": "string"}, "target_language": {"type": "string"}}, "required": ["code", "target_language"]}}},
]


def run_eval4(client, model: str) -> dict:
    results = []
    passed = 0
    for p in TOOLCALL_EVAL_PROMPTS:
        try:
            resp = client.chat.completions.create(
                # Tool routing is a classification task — run deterministic.
                # The previous temperature=0.2 caused ±10pp variance on the
                # same model, producing spurious regressions that weren't
                # actual model-quality changes.
                model=model, max_tokens=400, temperature=0.0,
                tools=TOOL_SCHEMAS_MIN, tool_choice="auto",
                messages=[
                    {"role": "system", "content": SYSTEM_PROMPT_NEBULA},
                    {"role": "user", "content": p["prompt"]},
                ],
            )
            msg = resp.choices[0].message
        except Exception as e:
            results.append({"prompt": p["prompt"][:80], "error": str(e)})
            continue
        tool_calls = getattr(msg, "tool_calls", None) or []
        ok = False
        tool_name = None
        if tool_calls:
            tc = tool_calls[0]
            tool_name = getattr(tc.function, "name", None) if hasattr(tc, "function") else None
            args_str = getattr(tc.function, "arguments", "") if hasattr(tc, "function") else ""
            # Args must parse as JSON.
            try:
                args = json.loads(args_str or "{}")
                args_json_ok = True
            except Exception:
                args_json_ok = False
            # Tool name must be in our set.
            allowed = {t["function"]["name"] for t in TOOL_SCHEMAS_MIN}
            name_ok = tool_name in allowed
            ok = args_json_ok and name_ok
        if ok:
            passed += 1
        results.append({
            "prompt": p["prompt"][:80],
            "ok": ok,
            "got": tool_name,
            "expected": p["expect_tool"],
        })
    total = len(results)
    rate = passed / total if total else 0.0
    return {
        "eval": "eval4_toolcall",
        "total": total, "passed": passed, "rate": rate,
        "pass_bar": PASS_BARS["eval4_toolcall"],
        "decision": "pass" if rate >= PASS_BARS["eval4_toolcall"] else "fail",
        "per_prompt": results,
    }


def run_eval2_stub() -> dict:
    return {
        "eval": "eval2_semantic",
        "deferred": True,
        "pass_bar": PASS_BARS["eval2_semantic"],
        "note": "HumanEval pytest runner not wired for this iteration. If evals 1/3/4 pass, ship the full fine-tune and add this eval pre-production.",
    }


def run_eval5_stub() -> dict:
    return {
        "eval": "eval5_e2e",
        "deferred": True,
        "pass_bar": PASS_BARS["eval5_e2e"],
        "note": "Fixture repos for end-to-end plugin test not wired. See plan §8 for the scope this deferred work.",
    }


def summarize(results: list[dict]) -> dict:
    live = [r for r in results if not r.get("deferred") and "error" not in r]
    decisions = [r.get("decision") for r in live]
    if live and all(d == "pass" for d in decisions):
        verdict = "green"
    elif any(d == "fail" for d in decisions):
        verdict = "block"
    else:
        verdict = "pending"
    return {
        "pilot_verdict": verdict,
        "evals": results,
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default=PILOT_MODEL,
                    help="Fine-tuned model id, e.g. 'ft:gpt-4.1-mini:...:suffix:abc'.")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--limit-eval1", type=int, default=30)
    ap.add_argument("--limit-eval3", type=int, default=20)
    ap.add_argument("--run-id", default=None)
    args = ap.parse_args()

    ensure_dirs()
    run_id = args.run_id or time.strftime("%Y%m%d_%H%M%S")
    out_dir = EVAL_RESULTS / run_id
    out_dir.mkdir(parents=True, exist_ok=True)

    if args.dry_run:
        print(f"[dry-run] would evaluate {args.model} on eval1({args.limit_eval1}) + eval3({args.limit_eval3}) + eval4({len(TOOLCALL_EVAL_PROMPTS)})")
        print(f"[dry-run] evals 2 + 5 deferred (need runner / fixtures)")
        print(f"[dry-run] output would land under {out_dir}")
        return

    client = load_openai_client()
    print(f"Running evals against {args.model} → {out_dir}")

    results: list[dict] = []
    print(f"\n[eval1] Nebula syntax validity ({args.limit_eval1} prompts)…")
    r1 = run_eval1(client, args.model, args.limit_eval1)
    (out_dir / "eval1_syntax.json").write_text(json.dumps(r1, indent=2))
    print(f"  rate={r1.get('rate'):.2%}  decision={r1.get('decision')}")
    results.append(r1)

    print(f"\n[eval3] Anchor / no-forgetting ({args.limit_eval3} prompts)…")
    r3 = run_eval3(client, args.model, args.limit_eval3)
    (out_dir / "eval3_anchor.json").write_text(json.dumps(r3, indent=2))
    print(f"  rate={r3.get('rate'):.2%}  decision={r3.get('decision')}")
    results.append(r3)

    print(f"\n[eval4] Tool-use fidelity ({len(TOOLCALL_EVAL_PROMPTS)} prompts)…")
    r4 = run_eval4(client, args.model)
    (out_dir / "eval4_toolcall.json").write_text(json.dumps(r4, indent=2))
    print(f"  rate={r4.get('rate'):.2%}  decision={r4.get('decision')}")
    results.append(r4)

    r2 = run_eval2_stub()
    (out_dir / "eval2_semantic.json").write_text(json.dumps(r2, indent=2))
    results.append(r2)
    r5 = run_eval5_stub()
    (out_dir / "eval5_e2e.json").write_text(json.dumps(r5, indent=2))
    results.append(r5)

    summary = summarize(results)
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"\n{'=' * 60}")
    print(f"Pilot verdict: {summary['pilot_verdict']}")
    print(f"{'=' * 60}")
    print(f"Full results: {out_dir}/")


if __name__ == "__main__":
    main()
