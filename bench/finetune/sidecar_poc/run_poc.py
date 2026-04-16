#!/usr/bin/env python3
"""Phase 20 sidecar POC — zero-shot Qwen3-Coder-480B on (original_native,
modified_nebula) → modified_native, with the original native source as a
style reference.

Goal: gauge whether a fine-tuned sidecar model can reliably reconstruct
byte-exact native source from a lossy Nebula representation + the original.
If the teacher can do this zero-shot at ≥70% byte-exact, a distilled 7B
sidecar is very likely to work.

Data synthesis: for each .go file in the benchmark templates, we perform a
synthetic rename (Go identifier → same with a 'Renamed' suffix) on both the
declaration and all intra-file call sites. The modified file is ingested
via `nebula from-go` to produce the Nebula input. The teacher's job: output
the modified native file given the ORIGINAL native source and the modified
Nebula source.

Output: poc_results.jsonl with one row per triple, plus a summary table.
"""

from __future__ import annotations
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Optional

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
NEBULA = REPO / "compiler" / "target" / "release" / "nebula"
OUT_DIR = HERE / "runs"
OUT_DIR.mkdir(parents=True, exist_ok=True)

# Reuse the Together client already battle-tested in distill.py.
sys.path.insert(0, str(REPO / "bench" / "finetune" / "qwen3_coder_30b"))
from distill import _load_together_key, _together_chat  # type: ignore

TOGETHER_MODEL = "Qwen/Qwen3-Coder-480B-A35B-Instruct-FP8"

# ── Synthetic transform ──────────────────────────────────────────────────

# Match top-level Go function declarations and methods (ignoring receiver).
FUNC_DECL_RE = re.compile(r"^func(?:\s+\([^)]*\))?\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", re.MULTILINE)


def pick_rename_target(src: str) -> Optional[tuple[str, str]]:
    """Pick a function/method name to rename. Avoid test helpers and very short names."""
    candidates = []
    for m in FUNC_DECL_RE.finditer(src):
        name = m.group(1)
        if len(name) < 3:
            continue
        if name.startswith("Test") or name.startswith("Benchmark"):
            continue
        candidates.append(name)
    if not candidates:
        return None
    # Prefer the first candidate whose name only appears in this file in
    # ways we can safely substitute: `name(`, `.name(`, `name,`. Skip names
    # that collide with common builtins.
    skip = {"Len", "Error", "String", "Bytes", "Read", "Write", "Close"}
    for name in candidates:
        if name in skip:
            continue
        # `word` boundary replacement, only when followed by ( or ,
        return (name, f"{name}Renamed")
    return None


def apply_rename(src: str, old: str, new: str) -> str:
    # Replace only at call-ish positions: `oldname(`, `.oldname(`, `oldname,`,
    # and the function declaration line. This mirrors how the MCP's byte-level
    # patch_code would apply a package-wide rename in one file.
    def repl(match: re.Match) -> str:
        return match.group(0).replace(old, new)

    pattern = rf"(?<![A-Za-z0-9_])({re.escape(old)})(?=\s*[(,])"
    return re.sub(pattern, repl, src)


def ingest_go_to_nebula(go_src: str, work: Path) -> str:
    tmp = work / "in.go"
    tmp.write_text(go_src)
    out = subprocess.run(
        [str(NEBULA), "from-go", str(tmp)],
        capture_output=True, text=True, cwd=str(work),
    )
    if out.returncode != 0:
        raise RuntimeError(f"from-go failed: {out.stderr[:300]}")
    return out.stdout


# ── Teacher call ──────────────────────────────────────────────────────────

SYSTEM_PROMPT = """You apply diffs. You are NOT a translator.

You will receive:
- ORIGINAL: an authoritative native source file (Go, Python, etc.). This is the ground truth for formatting, whitespace, identifier casing, comments, import order, brace placement, and every byte of every unchanged line.
- MODIFIED (Nebula IL): a lossy canonical representation of what the modified file should look like semantically. Nebula uses snake_case identifiers, `:=` for immutables, indent blocks, and strips ceremony — all of which differ from the original's native form. Do NOT copy Nebula syntax into your output.

Your job: output the MODIFIED native source. Specifically:

1. Compute the semantic diff between the ORIGINAL and the MODIFIED (Nebula) form. Ignore syntactic differences that are just Nebula's canonical form — same meaning, same output. Focus only on real semantic changes: renamed identifiers, added/removed functions, changed signatures, changed function bodies, added/removed struct fields.
2. For every line of the ORIGINAL that doesn't correspond to a semantic change: copy it byte-for-byte into the output. Do not reformat. Do not add or remove blank lines. Do not touch comments. Preserve the exact indentation (tabs vs spaces).
3. For lines that DO correspond to a semantic change: write the new line in the ORIGINAL's native style (CamelCase, braces, `for i := 0; i < n; i++`, etc.), not Nebula's style. If the Nebula renames `keys` to `keys_renamed`, the native output should rename `Keys` to `KeysRenamed` (preserve the original's casing convention).
4. The output must end with exactly one trailing newline, matching the ORIGINAL's convention.

Output: the full modified native source file inside ONE fenced code block tagged with the original language. No explanations. No diff markers. No prose."""


FEW_SHOT = """Example 1:

ORIGINAL (go):
```go
package demo

// Add returns a + b.
func Add(a int, b int) int {
\treturn a + b
}
```

MODIFIED (Nebula IL):
```
def add_two(a: Int, b: Int) -> Int:
  return a + b
```

Modified go (correct answer — note `Add` renamed to `AddTwo`, comment and formatting preserved):
```go
package demo

// Add returns a + b.
func AddTwo(a int, b int) int {
\treturn a + b
}
```

Now do the real task:
"""

USER_TEMPLATE = """{few_shot}
Language: {lang}

ORIGINAL ({lang}):
```{lang}
{original}
```

MODIFIED (Nebula IL):
```
{nebula}
```

Output the full modified {lang} source file below, inside one ```{lang} block. Preserve every byte of every unchanged line from the ORIGINAL. Apply only the semantic changes described by the Nebula form:
"""

FENCE_RE = re.compile(r"```(?:go|python|py|)\s*\n(.*?)```", re.DOTALL)


def extract_code(text: str) -> Optional[str]:
    m = FENCE_RE.search(text)
    if m:
        return m.group(1)
    # Fall back: some responses emit without a fence.
    stripped = text.strip()
    if stripped.startswith("package ") or stripped.startswith("func "):
        return stripped
    return None


def call_teacher(api_key: str, original: str, nebula: str, lang: str) -> dict:
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": USER_TEMPLATE.format(
            lang=lang, original=original, nebula=nebula, few_shot=FEW_SHOT,
        )},
    ]
    t0 = time.time()
    resp = _together_chat(api_key, messages, tools=[])
    wall = time.time() - t0

    choice = resp.get("choices", [{}])[0]
    msg = choice.get("message", {}) or {}
    content = msg.get("content") or ""
    usage = resp.get("usage", {}) or {}

    return {
        "wall_seconds": round(wall, 2),
        "raw_text": content,
        "extracted": extract_code(content),
        "usage": usage,
        "finish_reason": choice.get("finish_reason"),
    }


# ── Scoring ───────────────────────────────────────────────────────────────

def score(expected: str, actual: Optional[str]) -> dict:
    if actual is None:
        return {"byte_exact": False, "trailing_eq": False, "normalized_eq": False, "len_delta": None}
    byte_exact = expected == actual
    # Trailing-whitespace tolerant: differ only in trailing newlines.
    trailing_eq = expected.rstrip() + "\n" == actual.rstrip() + "\n"
    # Normalized equality: collapse whitespace runs, strip trailing blanks.
    def norm(s: str) -> str:
        return "\n".join(re.sub(r"[ \t]+", " ", ln.rstrip()) for ln in s.splitlines()).rstrip() + "\n"
    normalized_eq = norm(expected) == norm(actual)
    return {
        "byte_exact": byte_exact,
        "trailing_eq": trailing_eq,
        "normalized_eq": normalized_eq,
        "len_delta": len(actual) - len(expected),
    }


# ── Main ──────────────────────────────────────────────────────────────────

def find_go_corpus() -> list[Path]:
    templates = REPO / "bench" / "mcp_manual_test"
    files = []
    for p in templates.glob("*_template/*.go"):
        if "_test.go" in p.name:
            continue
        files.append(p)
    files.sort()
    return files


def build_triple(go_path: Path, work: Path) -> Optional[dict]:
    original = go_path.read_text()
    rename = pick_rename_target(original)
    if not rename:
        return None
    old, new = rename
    modified = apply_rename(original, old, new)
    if modified == original:
        return None
    try:
        nebula = ingest_go_to_nebula(modified, work)
    except Exception as e:
        return {"error": f"ingest: {e}"}
    return {
        "file": str(go_path.relative_to(REPO)),
        "lang": "go",
        "rename": [old, new],
        "original": original,
        "expected_modified": modified,
        "modified_nebula": nebula,
    }


def main() -> None:
    max_calls = int(os.environ.get("POC_MAX", "25"))
    assert NEBULA.exists(), f"nebula binary missing at {NEBULA}"

    api_key = _load_together_key()
    if not api_key:
        print("TOGETHER_API_KEY not set", file=sys.stderr)
        sys.exit(1)

    corpus = find_go_corpus()
    print(f"Corpus: {len(corpus)} .go files found")

    work = OUT_DIR / "work"
    work.mkdir(exist_ok=True)

    triples = []
    for p in corpus:
        t = build_triple(p, work)
        if t and "error" not in t:
            triples.append(t)
    print(f"Built {len(triples)} triples")

    triples = triples[:max_calls]
    results_path = OUT_DIR / "poc_results.jsonl"
    print(f"Running {len(triples)} teacher calls → {results_path}")

    summary = {"n": 0, "byte_exact": 0, "trailing_eq": 0, "normalized_eq": 0, "extract_fail": 0}
    total_tokens_in = 0
    total_tokens_out = 0
    total_wall = 0.0

    with results_path.open("w") as out:
        for i, t in enumerate(triples, 1):
            print(f"[{i}/{len(triples)}] {t['file']}  rename={t['rename'][0]}→{t['rename'][1]}", flush=True)
            try:
                tr = call_teacher(api_key, t["original"], t["modified_nebula"], t["lang"])
            except Exception as e:
                print(f"  teacher error: {e}")
                continue
            sc = score(t["expected_modified"], tr["extracted"])
            total_tokens_in += int(tr["usage"].get("prompt_tokens", 0))
            total_tokens_out += int(tr["usage"].get("completion_tokens", 0))
            total_wall += tr["wall_seconds"]

            summary["n"] += 1
            summary["byte_exact"] += int(sc["byte_exact"])
            summary["trailing_eq"] += int(sc["trailing_eq"])
            summary["normalized_eq"] += int(sc["normalized_eq"])
            if tr["extracted"] is None:
                summary["extract_fail"] += 1

            # Save full expected + actual for post-mortem.
            fname = t["file"].replace("/", "_")
            (OUT_DIR / "work" / f"{fname}.expected").write_text(t["expected_modified"])
            (OUT_DIR / "work" / f"{fname}.actual").write_text(tr["extracted"] or "")
            (OUT_DIR / "work" / f"{fname}.nebula").write_text(t["modified_nebula"])
            out.write(json.dumps({
                "file": t["file"],
                "rename": t["rename"],
                "wall_s": tr["wall_seconds"],
                "finish_reason": tr["finish_reason"],
                "score": sc,
                "usage": tr["usage"],
                "extracted_preview": (tr["extracted"] or "")[:200],
            }) + "\n")
            out.flush()
            print(f"  → byte_exact={sc['byte_exact']} normalized_eq={sc['normalized_eq']} len_delta={sc['len_delta']} wall={tr['wall_seconds']}s")

    n = max(summary["n"], 1)
    print()
    print(f"=== POC summary (n={summary['n']}) ===")
    print(f"  byte_exact     : {summary['byte_exact']:3d}  ({100*summary['byte_exact']/n:5.1f}%)")
    print(f"  trailing_eq    : {summary['trailing_eq']:3d}  ({100*summary['trailing_eq']/n:5.1f}%)  (trailing-newline tolerant)")
    print(f"  normalized_eq  : {summary['normalized_eq']:3d}  ({100*summary['normalized_eq']/n:5.1f}%)")
    print(f"  extract_fail   : {summary['extract_fail']:3d}  ({100*summary['extract_fail']/n:5.1f}%)")
    print(f"  total tokens in/out: {total_tokens_in} / {total_tokens_out}")
    print(f"  total wall seconds : {total_wall:.1f}")
    print(f"  cost estimate (@ $0.22/$0.88 per 1M Together): ${(total_tokens_in*0.22 + total_tokens_out*0.88)/1e6:.4f}")
    (OUT_DIR / "poc_summary.json").write_text(json.dumps({
        **summary, "total_tokens_in": total_tokens_in, "total_tokens_out": total_tokens_out,
        "total_wall_s": total_wall,
    }, indent=2))


if __name__ == "__main__":
    main()
