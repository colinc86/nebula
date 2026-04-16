#!/usr/bin/env python3
"""Post-train eval harness.

Reruns the val set through a specified Together model (the fine-tuned
sidecar) and produces the same scoring breakdown as eval_val.py — but
against the new model instead of zero-shot 480B.

Usage:
    .venv/bin/python3 eval_finetune.py <model_id> [n_samples]

Compares against the zero-shot baseline grade (55% normalized_eq) saved
in runs/val_summary.json.
"""

from __future__ import annotations
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
sys.path.insert(0, str(REPO / "bench" / "finetune" / "qwen3_coder_30b"))
import distill  # type: ignore
from distill import _load_together_key, _together_chat  # type: ignore

FENCE_RE = re.compile(r"```(?:go|py|python|)\s*\n(.*?)```", re.DOTALL)


def extract_code(text: str) -> str | None:
    m = FENCE_RE.search(text)
    if m:
        return m.group(1)
    s = text.strip()
    if s.startswith("package ") or s.startswith("def ") or s.startswith("import "):
        return s
    return None


def extract_expected(assistant_text: str) -> str:
    m = FENCE_RE.search(assistant_text)
    return m.group(1) if m else assistant_text


def score(expected: str, actual: str | None) -> dict:
    if actual is None:
        return {"byte_exact": False, "trailing_eq": False, "normalized_eq": False}
    byte_exact = expected == actual
    trailing_eq = expected.rstrip() + "\n" == actual.rstrip() + "\n"
    def norm(s: str) -> str:
        return "\n".join(re.sub(r"[ \t]+", " ", ln.rstrip()) for ln in s.splitlines()).rstrip() + "\n"
    return {
        "byte_exact": byte_exact,
        "trailing_eq": trailing_eq,
        "normalized_eq": norm(expected) == norm(actual),
    }


def main() -> None:
    if len(sys.argv) < 2:
        print("usage: eval_finetune.py <model_id> [n_samples]", file=sys.stderr)
        sys.exit(1)
    model_id = sys.argv[1]
    n_samples = int(sys.argv[2]) if len(sys.argv) > 2 else None
    api_key = _load_together_key()

    val_path = HERE / "data" / "sidecar_val.jsonl"
    rows = [json.loads(l) for l in val_path.open()]
    if n_samples:
        rows = rows[:n_samples]
    print(f"Evaluating {len(rows)} val triples on {model_id}")

    by_lang = defaultdict(lambda: {"n": 0, "byte": 0, "trail": 0, "norm": 0})
    by_edit = defaultdict(lambda: {"n": 0, "byte": 0, "trail": 0, "norm": 0})
    total_in = total_out = 0
    results = []

    saved_model = distill.TOGETHER_MODEL
    distill.TOGETHER_MODEL = model_id
    try:
        for i, r in enumerate(rows, 1):
            meta = r.get("_meta", {})
            expected = extract_expected(r["messages"][2]["content"])
            eval_messages = [r["messages"][0], r["messages"][1]]
            try:
                resp = _together_chat(api_key, eval_messages, tools=[])
            except Exception as e:
                print(f"  [{i}] error: {e}")
                continue
            text = (resp.get("choices", [{}])[0].get("message", {}) or {}).get("content", "") or ""
            usage = resp.get("usage", {}) or {}
            total_in += int(usage.get("prompt_tokens", 0))
            total_out += int(usage.get("completion_tokens", 0))
            actual = extract_code(text)
            sc = score(expected, actual)
            lang = meta.get("lang", "?"); edit = meta.get("edit_type", "?")
            for bucket in (by_lang[lang], by_edit[edit]):
                bucket["n"] += 1
                bucket["byte"] += int(sc["byte_exact"])
                bucket["trail"] += int(sc["trailing_eq"])
                bucket["norm"] += int(sc["normalized_eq"])
            results.append({"meta": meta, "score": sc})
            if i % 25 == 0 or i == len(rows):
                print(f"  [{i}/{len(rows)}] running tokens in/out: {total_in}/{total_out}", flush=True)
    finally:
        distill.TOGETHER_MODEL = saved_model

    out = HERE / "runs" / f"eval_{model_id.replace('/', '_').replace(':', '_')}.jsonl"
    out.parent.mkdir(exist_ok=True)
    with out.open("w") as fh:
        for r in results:
            fh.write(json.dumps(r) + "\n")

    n = len(results)
    byte = sum(r["score"]["byte_exact"] for r in results)
    trail = sum(r["score"]["trailing_eq"] for r in results)
    norm = sum(r["score"]["normalized_eq"] for r in results)
    def pct(x): return f"{100*x/max(1,n):5.1f}%"
    print()
    print(f"=== Eval summary (model={model_id}, n={n}) ===")
    print(f"  byte_exact     : {byte:4d}  ({pct(byte)})")
    print(f"  trailing_eq    : {trail:4d}  ({pct(trail)})")
    print(f"  normalized_eq  : {norm:4d}  ({pct(norm)})")
    print(f"  tokens in/out  : {total_in} / {total_out}")
    print()
    print("By language:")
    for k, v in sorted(by_lang.items()):
        n_ = v["n"] or 1
        print(f"  {k:8s} n={v['n']:4d}  byte={100*v['byte']/n_:4.0f}%  trail={100*v['trail']/n_:4.0f}%  norm={100*v['norm']/n_:4.0f}%")
    print()
    print("By edit type:")
    for k, v in sorted(by_edit.items()):
        n_ = v["n"] or 1
        print(f"  {k:25s} n={v['n']:4d}  byte={100*v['byte']/n_:4.0f}%  trail={100*v['trail']/n_:4.0f}%  norm={100*v['norm']/n_:4.0f}%")

    # Compare to baseline
    baseline_path = HERE / "runs" / "val_summary.json"
    if baseline_path.exists():
        b = json.loads(baseline_path.read_text())
        n_b = max(b.get("n", 1), 1)
        print()
        print("Baseline (zero-shot 480B):")
        print(f"  byte_exact   {100*b.get('byte', 0)/n_b:5.1f}%")
        print(f"  trailing_eq  {100*b.get('trail', 0)/n_b:5.1f}%")
        print(f"  normalized_eq{100*b.get('norm', 0)/n_b:5.1f}%")
        print()
        delta_norm = 100*norm/max(1,n) - 100*b.get("norm", 0)/n_b
        print(f"Δ normalized_eq vs baseline: {delta_norm:+.1f}pp")
        if delta_norm >= 30:
            print("✅ STRONG IMPROVEMENT — proceed to Phase 21 / production")
        elif delta_norm >= 10:
            print("⚠️  marginal improvement — consider more training data or a larger base")
        else:
            print("❌ no meaningful improvement — investigate before scaling")


if __name__ == "__main__":
    main()
