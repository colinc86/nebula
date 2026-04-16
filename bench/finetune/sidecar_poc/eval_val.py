#!/usr/bin/env python3
"""Zero-shot eval on the val split.

Runs Qwen3-Coder-480B on each val triple via Together and scores against the
expected assistant message. Produces per-language and per-edit-type breakdown.
Purpose: establish the baseline grade the fine-tuned 7B sidecar must beat.
"""

from __future__ import annotations
import json
import re
import sys
import time
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
sys.path.insert(0, str(REPO / "bench" / "finetune" / "qwen3_coder_30b"))
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
    """The training assistant message is a fenced block `lang\n...```\n`. Strip the fence."""
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
    val_path = HERE / "data" / "sidecar_val.jsonl"
    rows = [json.loads(l) for l in val_path.open()]
    max_n = int(sys.argv[1]) if len(sys.argv) > 1 else len(rows)
    rows = rows[:max_n]
    print(f"Evaluating {len(rows)} val triples on Qwen3-Coder-480B (zero-shot)")

    api_key = _load_together_key()
    if not api_key:
        print("TOGETHER_API_KEY not set", file=sys.stderr); sys.exit(1)

    results = []
    total_in = total_out = 0
    by_lang = defaultdict(lambda: {"n": 0, "byte": 0, "trail": 0, "norm": 0})
    by_edit = defaultdict(lambda: {"n": 0, "byte": 0, "trail": 0, "norm": 0})

    for i, r in enumerate(rows, 1):
        meta = r.get("_meta", {})
        expected = extract_expected(r["messages"][2]["content"])
        # Strip the assistant turn from the training payload — we're evaluating, not teacher-forcing.
        eval_messages = [r["messages"][0], r["messages"][1]]
        try:
            resp = _together_chat(api_key, eval_messages, tools=[])
        except Exception as e:
            print(f"  [{i}] teacher error: {e}")
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
            print(f"  [{i}/{len(rows)}] running total in/out tok: {total_in}/{total_out}", flush=True)

    out_path = HERE / "runs" / "val_results.jsonl"
    out_path.parent.mkdir(exist_ok=True)
    with out_path.open("w") as fh:
        for r in results:
            fh.write(json.dumps(r) + "\n")

    n = len(results)
    byte = sum(r["score"]["byte_exact"] for r in results)
    trail = sum(r["score"]["trailing_eq"] for r in results)
    norm = sum(r["score"]["normalized_eq"] for r in results)

    def pct(x): return f"{100*x/max(1,n):5.1f}%"
    print()
    print(f"=== Val eval summary (n={n}) ===")
    print(f"  byte_exact     : {byte:4d}  ({pct(byte)})")
    print(f"  trailing_eq    : {trail:4d}  ({pct(trail)})")
    print(f"  normalized_eq  : {norm:4d}  ({pct(norm)})")
    print(f"  tokens in/out  : {total_in} / {total_out}")
    cost = (total_in * 0.22 + total_out * 0.88) / 1e6
    print(f"  cost estimate  : ${cost:.4f}")
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
    (HERE / "runs" / "val_summary.json").write_text(json.dumps({
        "n": n, "byte": byte, "trail": trail, "norm": norm,
        "tokens_in": total_in, "tokens_out": total_out,
        "by_lang": dict(by_lang), "by_edit": dict(by_edit),
    }, indent=2))


if __name__ == "__main__":
    main()
