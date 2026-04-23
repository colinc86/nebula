"""Slice 2 REFRAME — build instruction→Nebula rows from existing datasets
with zero external LLM calls.

Three sources, combined:

A. 203k reframing (~1,000 rows): for each row in
   `electrocampbell/nebula-8lang-203k` (user=Nebula, assistant=Native),
   synthesize a natural-language prompt from the native side (function
   name + signature + docstring where present) and pair with the stored
   Nebula. Prompt synthesis is pure string manipulation — no model.

B. HumanEval canonical solutions (~100 rows): for each HumanEval row not
   in the eval 2 held-out set, extract the docstring description as a
   prompt and run `canonical_solution` through `nebula from-python` to
   produce the target. Pure compiler, no LLM.

C. MBPP canonical solutions (optional, skipped if file absent).

Combined with `slice2_direct.py`'s curated-drill output (~114 rows), this
should hit the 1,500 slice-2 target without any API spend.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from config import (
    SYSTEM_PROMPT_NEBULA, SLICE_FILES, REJECTED_FILES, LANG_EXT, LANG_DISPLAY,
    MIN_NEBULA_LEN, MAX_NEBULA_LEN, ensure_dirs, REPO,
)
import nebula_cli


HUMAN_EVAL_FILE = REPO / "bench" / "corpus" / "humaneval_py" / "humaneval.jsonl"
HUMAN_EVAL_HOLDOUT = set(f"HumanEval/{i}" for i in range(0, 50))


SYSTEM_LANG_HINTS = {
    "Python": "python", "JavaScript": "javascript", "TypeScript": "typescript",
    "Go": "go", "Swift": "swift", "Kotlin": "kotlin", "Rust": "rust", "C": "c",
}


def _lang_from_system(system_content: str) -> str | None:
    m = re.search(r"idiomatic (\w+) code", system_content or "")
    if not m:
        return None
    return SYSTEM_LANG_HINTS.get(m.group(1))


# ── Prompt synthesis from native code (no LLM) ────────────────────────

def extract_python_docstring(src: str) -> str | None:
    """Return the first triple-quoted docstring in the src, normalized."""
    m = re.search(r'"""(.*?)"""', src, re.DOTALL)
    if m:
        doc = m.group(1).strip()
        doc = re.sub(r"^\s*>>> .*$", "", doc, flags=re.MULTILINE).strip()
        if doc and len(doc) >= 20:
            return doc
    return None


def extract_leading_comment_block(src: str, comment_char: str = "//") -> str | None:
    lines: list[str] = []
    for raw in src.splitlines():
        s = raw.strip()
        if not s:
            if lines: break
            continue
        if s.startswith(comment_char):
            lines.append(s.lstrip(comment_char).strip())
        else:
            break
    doc = " ".join(lines).strip()
    return doc if len(doc) >= 20 else None


def extract_signature(src: str, lang: str) -> str | None:
    """Pull the first function signature (name + arglist) from the source."""
    patterns = {
        "python":     r"(?:^|\n)\s*def\s+(\w+)\s*\(([^)]*)\)",
        "javascript": r"(?:^|\n)\s*(?:async\s+)?function\s+(\w+)\s*\(([^)]*)\)",
        "typescript": r"(?:^|\n)\s*(?:async\s+)?function\s+(\w+)\s*\(([^)]*)\)",
        "go":         r"(?:^|\n)\s*func\s+(?:\([^)]*\)\s*)?(\w+)\s*\(([^)]*)\)",
        "swift":      r"(?:^|\n)\s*func\s+(\w+)\s*\(([^)]*)\)",
        "kotlin":     r"(?:^|\n)\s*fun\s+(\w+)\s*\(([^)]*)\)",
        "rust":       r"(?:^|\n)\s*(?:pub\s+)?fn\s+(\w+)\s*\(([^)]*)\)",
        "c":          r"(?:^|\n)\s*\w+\s+(\w+)\s*\(([^)]*)\)\s*\{",
    }
    pat = patterns.get(lang)
    if not pat:
        return None
    m = re.search(pat, src)
    if not m:
        return None
    name = m.group(1)
    args = re.sub(r"\s+", " ", m.group(2)).strip()
    return f"{name}({args})"


def synthesize_prompt(native: str, lang: str) -> str | None:
    """Build a natural-language prompt from the native code. Prefers the
    function's docstring/comment; falls back to signature-only."""
    docstring = None
    if lang == "python":
        docstring = extract_python_docstring(native)
    elif lang in ("javascript", "typescript", "go", "swift", "kotlin", "rust", "c"):
        docstring = extract_leading_comment_block(native, "//")

    sig = extract_signature(native, lang)
    display = LANG_DISPLAY.get(lang, lang.title())

    if docstring and sig:
        return f"Write a function `{sig}` that {docstring}. Respond in Nebula IL."
    if docstring:
        return f"Write a function that {docstring}. Respond in Nebula IL."
    if sig:
        return f"Write a function equivalent to {display}'s `{sig}`. Respond in Nebula IL."
    return None


# ── Source A — 203k reframing ─────────────────────────────────────────

def pull_203k_reframed(target: int) -> tuple[list[dict], int]:
    """Stream the 203k dataset, synthesize prompts, return rows. Returns
    (rows, skipped_count)."""
    from datasets import load_dataset  # type: ignore
    ds = load_dataset("electrocampbell/nebula-8lang-203k", split="train", streaming=True)
    rows: list[dict] = []
    skipped = 0
    per_lang: dict[str, int] = {lang: 0 for lang in LANG_EXT.keys()}
    per_lang_target = max(1, target // len(LANG_EXT))
    seen = set()
    for idx, row in enumerate(ds):
        if len(rows) >= target:
            break
        msgs = row.get("messages") or []
        system = next((m for m in msgs if m.get("role") == "system"), None)
        user = next((m for m in msgs if m.get("role") == "user"), None)
        assistant = next((m for m in msgs if m.get("role") == "assistant"), None)
        if not (system and user and assistant):
            continue
        lang = _lang_from_system(system.get("content") or "")
        if lang not in LANG_EXT:
            continue
        if per_lang[lang] >= per_lang_target:
            continue
        native = assistant.get("content") or ""
        if not native.strip():
            skipped += 1
            continue
        # Track 4 iter-2: regenerate Nebula from native with the current
        # compiler. The stored HF `nebula` is stale (built with the pre-
        # Track-4 ingester bugs) and wouldn't train the model on the
        # shapes we actually want.
        nebula = nebula_cli.to_nebula(native, lang)
        if nebula is None or not nebula.strip():
            skipped += 1
            continue
        if not (MIN_NEBULA_LEN <= len(nebula) <= MAX_NEBULA_LEN):
            skipped += 1
            continue
        if not nebula_cli.check(nebula):
            skipped += 1
            continue
        prompt = synthesize_prompt(native, lang)
        if not prompt:
            skipped += 1
            continue
        prompt_hash = hashlib.sha1(prompt.encode()).hexdigest()
        if prompt_hash in seen:
            continue
        seen.add(prompt_hash)
        rows.append({
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT_NEBULA},
                {"role": "user", "content": prompt},
                {"role": "assistant", "content": f"```nebula\n{nebula}\n```"},
            ],
            "slice": "instruction",
            "source": f"203k-reframed#{idx}",
            "row_id": hashlib.sha1(f"instruction:203k:{idx}".encode()).hexdigest()[:16],
            "seed_native_lang": lang,
        })
        per_lang[lang] += 1
    return rows, skipped


# ── Source B — HumanEval canonical solutions → Nebula ─────────────────

def pull_humaneval() -> tuple[list[dict], int]:
    rows: list[dict] = []
    skipped = 0
    if not HUMAN_EVAL_FILE.exists():
        return rows, skipped
    for line in HUMAN_EVAL_FILE.open():
        line = line.strip()
        if not line:
            continue
        try:
            r = json.loads(line)
        except Exception:
            continue
        task_id = r.get("task_id")
        if task_id in HUMAN_EVAL_HOLDOUT:
            continue
        # Prompt = docstring description.
        raw_prompt = r.get("prompt", "")
        docstring = extract_python_docstring(raw_prompt)
        if not docstring:
            skipped += 1
            continue
        # Assembled Python = header + canonical_solution.
        full_py = raw_prompt + r.get("canonical_solution", "")
        # Compile through nebula.
        nebula = nebula_cli.to_nebula(full_py, "python")
        if nebula is None or not nebula_cli.check(nebula):
            skipped += 1
            continue
        if not (MIN_NEBULA_LEN <= len(nebula) <= MAX_NEBULA_LEN):
            skipped += 1
            continue
        name = r.get("entry_point", "the_function")
        prompt = f"Write a function `{name}` that {docstring}. Respond in Nebula IL."
        rows.append({
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT_NEBULA},
                {"role": "user", "content": prompt},
                {"role": "assistant", "content": f"```nebula\n{nebula}\n```"},
            ],
            "slice": "instruction",
            "source": f"humaneval:{task_id}",
            "row_id": hashlib.sha1(f"instruction:humaneval:{task_id}".encode()).hexdigest()[:16],
            "seed_native_lang": "python",
        })
    return rows, skipped


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--n-203k", type=int, default=1000)
    ap.add_argument("--append", action="store_true",
                    help="Append to existing slice2_instruction.jsonl instead of overwriting.")
    args = ap.parse_args()

    ensure_dirs()

    if args.dry_run:
        print(f"[dry-run] would pull up to {args.n_203k} rows from 203k-reframed")
        print(f"[dry-run] would pull up to ~{164 - len(HUMAN_EVAL_HOLDOUT)} rows from HumanEval canonical")
        return

    print(f"Pulling {args.n_203k} rows from 203k reframing…")
    a_rows, a_skipped = pull_203k_reframed(args.n_203k)
    print(f"  kept {len(a_rows)}, skipped {a_skipped}")

    print(f"Pulling HumanEval canonical (excl. holdout {sorted(HUMAN_EVAL_HOLDOUT)[:3]}…)…")
    b_rows, b_skipped = pull_humaneval()
    print(f"  kept {len(b_rows)}, skipped {b_skipped}")

    new_rows = a_rows + b_rows
    print(f"\nTotal new rows from reframing: {len(new_rows)}")

    out_path = SLICE_FILES["instruction"]
    rej_path = REJECTED_FILES["instruction"]
    out_path.parent.mkdir(parents=True, exist_ok=True)

    existing: list[dict] = []
    if args.append and out_path.exists():
        existing = [json.loads(l) for l in out_path.open() if l.strip()]
        print(f"Appending to existing {len(existing)} rows from {out_path.name}")

    # Dedup by row_id.
    seen_ids = {r.get("row_id") for r in existing}
    dedup_new = [r for r in new_rows if r.get("row_id") not in seen_ids]
    combined = existing + dedup_new

    with out_path.open("w") as f:
        for r in combined:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"Wrote {len(combined)} rows → {out_path}")


if __name__ == "__main__":
    main()
