"""Slice 4 DIVERGENCE — targeted "anti-idiom" training rows.

For each language we maintain a curated list of native-code snippets that
exercise divergence points — places where the native idiom differs from
Nebula's grammar (Go's `if x := f(); cond`, Rust turbofish `::<T>`, Swift's
`guard let ... else`, Kotlin's elvis `?:`, Python decorators, TS `T[]`,
JS destructure params, C typedef structs, etc.).

Each snippet becomes ONE training row:

    user:      "Write a function equivalent to <Lang>'s `<signature>`. Respond in Nebula IL."
    assistant: "```nebula\n<compiler output>\n```"

The assistant target is produced by running the snippet through
`nebula from-<lang>`. This guarantees the target reflects the compiler's
current rewrite rule — the model learns "see native idiom → emit clean
Nebula without the idiom."

Each target is validated against `nebula check` before inclusion. Rows
that fail check are dropped with a reason logged.

Output: build/slice4_divergence.jsonl (new slice).
"""
from __future__ import annotations
import hashlib
import json
import re
import sys
from pathlib import Path
from collections import Counter

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from config import SYSTEM_PROMPT_NEBULA, BUILD, LANG_DISPLAY, LANG_EXT, ensure_dirs
import nebula_cli


SOURCE_DIR = HERE / "divergence_source"
OUTPUT = BUILD / "slice4_divergence.jsonl"
REJECTS = BUILD / "slice4_divergence.rejected.jsonl"


# ── Prompt synthesis ──────────────────────────────────────────────────

def extract_signature(native: str, lang: str) -> str | None:
    """Pull the first function/struct signature out of the snippet. The
    synthesized prompt pins the model to a specific function name so we
    don't pollute training with mismatched generic prompts."""
    # Python: def name(params):
    if lang == "python":
        m = re.search(r"^(?:async\s+)?def\s+(\w+\([^)]*\))", native, re.MULTILINE)
        if m:
            return m.group(1)
        # Class declarations as fallback.
        m = re.search(r"^class\s+(\w+)", native, re.MULTILINE)
        if m:
            return m.group(1)
    # Rust: fn name<generics>?(params) -> ret
    if lang == "rust":
        m = re.search(r"\bfn\s+(\w+(?:<[^>]*>)?\s*\([^)]*\)(?:\s*->\s*[^{]+?)?)\s*\{", native)
        if m:
            return m.group(1).strip()
        m = re.search(r"\b(?:struct|enum|trait)\s+(\w+(?:<[^>]*>)?)", native)
        if m:
            return m.group(1)
    # Go: func (recv) name(params) ret
    if lang == "go":
        m = re.search(r"\bfunc\s+(?:\([^)]*\)\s+)?(\w+\s*\([^)]*\)(?:\s*\([^)]*\)|\s*[^ {]*)?)\s*\{", native)
        if m:
            return m.group(1).strip()
        m = re.search(r"\btype\s+(\w+)\s+(?:struct|interface)", native)
        if m:
            return m.group(1)
    # Swift / Kotlin: func / fun name(params) -> ret / : ret
    if lang in ("swift", "kotlin"):
        kw = "func" if lang == "swift" else "fun"
        m = re.search(r"\b" + kw + r"\s+(\w+(?:<[^>]*>)?\s*\([^)]*\)(?:[^{]+?)?)\s*\{", native)
        if m:
            return m.group(1).strip()
        m = re.search(r"\b(?:class|struct|enum|interface|protocol)\s+(\w+)", native)
        if m:
            return m.group(1)
    # TypeScript / JavaScript
    if lang in ("typescript", "javascript"):
        m = re.search(r"\bfunction\s+(\w+\s*<[^>]*>?\s*\([^)]*\)(?:\s*:\s*[^{]+?)?)\s*\{", native)
        if m:
            return m.group(1).strip()
        m = re.search(r"\b(?:const|let|var)\s+(\w+)\s*=", native)
        if m:
            return m.group(1)
        m = re.search(r"\b(?:class|interface|type|enum)\s+(\w+)", native)
        if m:
            return m.group(1)
    # C: type name(params) or typedef
    if lang == "c":
        m = re.search(r"^[\w\s*]+?\s+(\w+\s*\([^)]*\))\s*\{", native, re.MULTILINE)
        if m:
            return m.group(1).strip()
        m = re.search(r"\btypedef\s+(?:struct|enum)\s*[\w\s{]*?\}\s*(\w+)\s*;", native, re.DOTALL)
        if m:
            return m.group(1)
        m = re.search(r"\btypedef\s+.*?\(\*(\w+)\)", native)
        if m:
            return m.group(1)
    return None


def build_prompt(native: str, lang: str, pattern: str) -> str:
    """Generate a short natural-language prompt. We keep prompts varied so
    the model doesn't just pattern-match on a single prompt template."""
    sig = extract_signature(native, lang)
    display = LANG_DISPLAY.get(lang, lang.title())

    # Three prompt templates, cycled by pattern hash for determinism.
    h = int(hashlib.sha1((lang + pattern + native[:80]).encode()).hexdigest(), 16) % 3
    if sig:
        if h == 0:
            return f"Write a function equivalent to {display}'s `{sig}`. Respond in Nebula IL."
        if h == 1:
            return f"Port this {display} to Nebula IL:\n\n```{LANG_EXT[lang]}\n{native}\n```"
        return f"Translate this {display} snippet into Nebula IL:\n\n```{LANG_EXT[lang]}\n{native}\n```"
    # No signature — fall back to full-snippet prompt.
    return f"Translate this {display} code into Nebula IL:\n\n```{LANG_EXT[lang]}\n{native}\n```"


# ── Row builder ───────────────────────────────────────────────────────

def build_row(native: str, lang: str, pattern: str, idx: int) -> tuple[dict | None, str]:
    """Returns (row_dict, reason). reason is 'ok' on success, or an error."""
    nebula = nebula_cli.to_nebula(native, lang)
    if nebula is None or not nebula.strip():
        return None, "compiler produced empty output"
    if not nebula_cli.check(nebula):
        return None, "regenerated Nebula failed `nebula check`"
    # Minimum length — reject trivial outputs that won't teach the model
    # anything.
    if len(nebula) < 15:
        return None, f"output too short ({len(nebula)} chars)"
    prompt = build_prompt(native, lang, pattern)
    assistant = f"```nebula\n{nebula}\n```"
    row_id = hashlib.sha1(f"divergence:{lang}:{pattern}:{idx}".encode()).hexdigest()[:16]
    return (
        {
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT_NEBULA},
                {"role": "user", "content": prompt},
                {"role": "assistant", "content": assistant},
            ],
            "slice": "divergence",
            "source": f"divergence:{lang}:{pattern}",
            "row_id": row_id,
            "seed_native_lang": lang,
            "pattern": pattern,
        },
        "ok",
    )


# ── Main ──────────────────────────────────────────────────────────────

def main() -> None:
    ensure_dirs()
    rows: list[dict] = []
    rejects: list[dict] = []
    per_lang: Counter = Counter()
    per_pattern: Counter = Counter()

    for src_file in sorted(SOURCE_DIR.glob("*.jsonl")):
        lang = src_file.stem
        if lang not in LANG_EXT:
            print(f"skip unknown lang file: {src_file.name}", file=sys.stderr)
            continue
        entries = []
        for line in src_file.open():
            if not line.strip():
                continue
            entries.append(json.loads(line))
        print(f"{lang}: {len(entries)} source snippets")
        for i, e in enumerate(entries):
            row, reason = build_row(e["native"], lang, e.get("pattern", "misc"), i)
            if row is None:
                rejects.append({
                    "lang": lang,
                    "pattern": e.get("pattern", "misc"),
                    "native_snippet": e["native"][:100],
                    "reason": reason,
                })
            else:
                rows.append(row)
                per_lang[lang] += 1
                per_pattern[f"{lang}:{e.get('pattern','misc')}"] += 1

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    with OUTPUT.open("w") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    with REJECTS.open("w") as f:
        for r in rejects:
            f.write(json.dumps(r) + "\n")

    print(f"\nWrote {len(rows)} rows → {OUTPUT}")
    print(f"Wrote {len(rejects)} rejects → {REJECTS}")
    print("\nPer-lang:")
    for k, v in sorted(per_lang.items(), key=lambda kv: -kv[1]):
        print(f"  {v:4}  {k}")
    if rejects:
        from collections import Counter as C
        by_reason = C(r["reason"][:60] for r in rejects)
        print("\nReject reasons (top):")
        for k, v in by_reason.most_common(10):
            print(f"  {v:4}  {k}")


if __name__ == "__main__":
    main()
