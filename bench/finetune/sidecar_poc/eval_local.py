#!/usr/bin/env python3
"""Eval the locally-deployed fine-tuned sidecar against the val set.

Same scoring as eval_finetune.py but talks to a local Ollama endpoint
instead of Together. Used to confirm that the GGUF/native deployment
matches the metrics we got from the Together dedicated endpoint.
"""

from __future__ import annotations
import json
import os
import re
import sys
from collections import defaultdict
from pathlib import Path

import urllib.request

HERE = Path(__file__).parent.resolve()
OLLAMA_URL = os.environ.get("NEBULA_SIDECAR_OLLAMA_URL", "http://192.168.68.94:11435")
MODEL = os.environ.get("NEBULA_SIDECAR_MODEL", "nebula-sidecar-7b")

FENCE_RE = re.compile(r"```(?:go|py|python|)\s*\n(.*?)```", re.DOTALL)


def extract_code(text: str) -> str | None:
    m = FENCE_RE.search(text)
    if m:
        return m.group(1)
    s = text.strip()
    if s.startswith("package ") or s.startswith("def "):
        return s
    return None


def extract_expected(asst: str) -> str:
    m = FENCE_RE.search(asst)
    return m.group(1) if m else asst


def score(expected: str, actual: str | None) -> dict:
    if actual is None:
        return {"byte_exact": False, "trailing_eq": False, "normalized_eq": False}
    byte_exact = expected == actual
    trailing_eq = expected.rstrip() + "\n" == actual.rstrip() + "\n"
    def norm(s: str) -> str:
        return "\n".join(re.sub(r"[ \t]+", " ", ln.rstrip()) for ln in s.splitlines()).rstrip() + "\n"
    return {"byte_exact": byte_exact, "trailing_eq": trailing_eq,
            "normalized_eq": norm(expected) == norm(actual)}


def call_ollama(messages: list) -> str:
    body = json.dumps({
        "model": MODEL, "messages": messages, "stream": False,
        "options": {"temperature": 0.05, "num_predict": 4096, "num_ctx": 16384},
    }).encode()
    req = urllib.request.Request(
        f"{OLLAMA_URL.rstrip('/')}/api/chat",
        data=body, headers={"Content-Type": "application/json"}, method="POST",
    )
    with urllib.request.urlopen(req, timeout=300) as resp:
        data = json.loads(resp.read())
    return (data.get("message") or {}).get("content", "") or ""


def main() -> None:
    n_samples = int(sys.argv[1]) if len(sys.argv) > 1 else 60
    val_path = HERE / "data" / "sidecar_val.jsonl"
    rows = [json.loads(l) for l in val_path.open()][:n_samples]
    print(f"Evaluating {len(rows)} val triples on {MODEL} @ {OLLAMA_URL}")

    by_lang = defaultdict(lambda: {"n": 0, "byte": 0, "norm": 0})
    by_edit = defaultdict(lambda: {"n": 0, "byte": 0, "norm": 0})
    results = []
    for i, r in enumerate(rows, 1):
        meta = r.get("_meta", {})
        expected = extract_expected(r["messages"][2]["content"])
        try:
            text = call_ollama([r["messages"][0], r["messages"][1]])
        except Exception as e:
            print(f"  [{i}] error: {e}"); continue
        actual = extract_code(text)
        sc = score(expected, actual)
        for bucket in (by_lang[meta.get("lang", "?")], by_edit[meta.get("edit_type", "?")]):
            bucket["n"] += 1
            bucket["byte"] += int(sc["byte_exact"])
            bucket["norm"] += int(sc["normalized_eq"])
        results.append({"meta": meta, "score": sc})
        if i % 10 == 0 or i == len(rows):
            print(f"  [{i}/{len(rows)}]", flush=True)

    n = len(results)
    byte = sum(r["score"]["byte_exact"] for r in results)
    norm = sum(r["score"]["normalized_eq"] for r in results)
    def pct(x): return f"{100*x/max(1,n):5.1f}%"
    print()
    print(f"=== Local sidecar eval (n={n}) ===")
    print(f"  byte_exact     : {byte:4d}  ({pct(byte)})")
    print(f"  normalized_eq  : {norm:4d}  ({pct(norm)})")
    print()
    print("By language:")
    for k, v in sorted(by_lang.items()):
        n_ = v["n"] or 1
        print(f"  {k:8s} n={v['n']:4d}  byte={100*v['byte']/n_:5.1f}%  norm={100*v['norm']/n_:5.1f}%")
    print()
    print("By edit type:")
    for k, v in sorted(by_edit.items()):
        n_ = v["n"] or 1
        print(f"  {k:25s} n={v['n']:4d}  byte={100*v['byte']/n_:5.1f}%  norm={100*v['norm']/n_:5.1f}%")
    out = HERE / "runs" / "local_eval.jsonl"
    out.parent.mkdir(exist_ok=True)
    with out.open("w") as fh:
        for r in results:
            fh.write(json.dumps(r) + "\n")


if __name__ == "__main__":
    main()
