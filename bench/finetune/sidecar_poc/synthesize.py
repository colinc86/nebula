#!/usr/bin/env python3
"""Phase 20 sidecar training-data synthesizer.

**SCOPE — THIS IS A TEST ON GO AND PYTHON ONLY.** The Nebula IL supports 8
languages (Python, JavaScript, TypeScript, Go, Swift, Kotlin, Rust, C), and the
eventual sidecar must handle all of them. For this POC we synthesize training
triples only for Go and Python so we can validate the architecture at minimum
cost before scaling. Once the 7B distillation proves the approach works, a
follow-up task (see `project_sidecar_languages.md` / task list) will expand the
synthesizer to JS/TS/Rust/Swift/Kotlin/C ingesters and retrain.

For each source file in the corpus, apply multiple synthetic edits and emit
a JSONL of (original_native, modified_nebula) → modified_native triples.

Output format mirrors Together's chat fine-tuning schema:
  {"messages": [
     {"role": "system", "content": SYSTEM},
     {"role": "user",   "content": "ORIGINAL...\\n\\nMODIFIED (Nebula IL)..."},
     {"role": "assistant", "content": "```go\\n...\\n```"}
  ]}

Edit types (Go):
  1. rename_func_go    rename top-level function + intra-file call sites
  2. rename_method_go  rename a method on a struct
  3. add_param_go      add an `ignored int` parameter (default value 0)
  4. rename_const_go   rename a package-level constant
  5. insert_comment_go add a // comment line inside a function body

Nebula produced by the actual `from-go` ingester, so training distribution
matches deployment distribution.
"""

from __future__ import annotations
import json
import os
import random
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Optional

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
NEBULA = REPO / "compiler" / "target" / "release" / "nebula"
assert NEBULA.exists(), f"nebula binary missing at {NEBULA}"

OUT_DIR = HERE / "data"
OUT_DIR.mkdir(parents=True, exist_ok=True)

random.seed(42)

# ── Prompt scaffolding (must match the POC prompt exactly) ────────────────

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

USER_TEMPLATE = """Language: {lang}

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


# ── Edit primitives (Go) ──────────────────────────────────────────────────

FUNC_DECL_GO_RE = re.compile(
    r"^func(?:\s+\([^)]*\))?\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", re.MULTILINE,
)
METHOD_DECL_GO_RE = re.compile(
    r"^func\s+\(\s*\w+\s+\*?(\w+)\s*\)\s+(\w+)\s*\(", re.MULTILINE,
)
CONST_DECL_GO_RE = re.compile(
    r"^const\s+([A-Za-z_][A-Za-z0-9_]*)\s*=", re.MULTILINE,
)
FUNC_BODY_START_RE = re.compile(r"^func[^\n]*\{\s*\n", re.MULTILINE)

GO_BUILTIN_SKIP = {
    "Len", "Error", "String", "Bytes", "Read", "Write", "Close",
    "Init", "New", "Get", "Set", "Add", "Sub", "Mul", "Div",
    "Marshal", "Unmarshal",
}


def _pick_renamable(src: str, pattern: re.Pattern, group: int = 1) -> Optional[str]:
    names = []
    for m in pattern.finditer(src):
        n = m.group(group)
        if len(n) < 3 or n.startswith("Test") or n.startswith("Benchmark"):
            continue
        if n in GO_BUILTIN_SKIP:
            continue
        names.append(n)
    return names[0] if names else None


def _safe_substitute(src: str, old: str, new: str, follow_pattern: str = r"\s*[(,]") -> str:
    pattern = rf"(?<![A-Za-z0-9_])({re.escape(old)})(?={follow_pattern})"
    return re.sub(pattern, lambda m: m.group(0).replace(old, new), src)


def edit_rename_func_go(src: str) -> Optional[str]:
    name = _pick_renamable(src, FUNC_DECL_GO_RE)
    if not name:
        return None
    new_name = f"{name}Fresh"
    modified = _safe_substitute(src, name, new_name)
    return modified if modified != src else None


def edit_rename_method_go(src: str) -> Optional[str]:
    for m in METHOD_DECL_GO_RE.finditer(src):
        _, method = m.group(1), m.group(2)
        if len(method) < 3 or method in GO_BUILTIN_SKIP:
            continue
        new_name = f"{method}V2"
        modified = _safe_substitute(src, method, new_name, follow_pattern=r"\s*\(")
        if modified != src:
            return modified
    return None


def edit_rename_const_go(src: str) -> Optional[str]:
    name = _pick_renamable(src, CONST_DECL_GO_RE)
    if not name:
        return None
    # For constants, call sites appear bare (not followed by `(`). Match word
    # boundary only.
    new_name = f"{name}Const"
    pattern = rf"(?<![A-Za-z0-9_])({re.escape(name)})(?![A-Za-z0-9_])"
    modified = re.sub(pattern, lambda m: m.group(0).replace(name, new_name), src)
    return modified if modified != src else None


def edit_add_param_go(src: str) -> Optional[str]:
    # Find a function signature with empty or short parameter list, append ", ignored int"
    # Only touch the declaration, not call sites (would break build but we don't require
    # that — the expected output matches what we produce).
    def add_param(match: re.Match) -> str:
        orig = match.group(0)
        # orig looks like: "func Foo(a int) int {" or "func Foo() {"
        # Find the paren group and append to it
        m = re.match(r"(func(?:\s+\([^)]*\))?\s+\w+\s*\()([^)]*)(\)\s*.*)", orig, re.DOTALL)
        if not m:
            return orig
        prefix, params, suffix = m.group(1), m.group(2), m.group(3)
        if "ignored" in params:
            return orig
        new_params = (params + ", ignored int").lstrip(", ") if params.strip() else "ignored int"
        return prefix + new_params + suffix

    # Match the whole function-signature line
    sig_line_re = re.compile(
        r"^func(?:\s+\([^)]*\))?\s+\w+\s*\([^)]*\)(?:\s*\([^)]*\))?(?:\s+[\w\[\].*<-]+)*\s*\{",
        re.MULTILINE,
    )
    m = sig_line_re.search(src)
    if not m:
        return None
    modified = src[: m.start()] + add_param(m) + src[m.end():]
    return modified if modified != src else None


def edit_insert_comment_go(src: str) -> Optional[str]:
    # Insert a "// note: preserved by sidecar" comment after the first "func ... {"
    m = FUNC_BODY_START_RE.search(src)
    if not m:
        return None
    insertion = "\t// note: preserved by sidecar\n"
    modified = src[: m.end()] + insertion + src[m.end():]
    return modified if modified != src else None


GO_EDITS: list[tuple[str, Callable[[str], Optional[str]]]] = [
    ("rename_func_go", edit_rename_func_go),
    ("rename_method_go", edit_rename_method_go),
    ("rename_const_go", edit_rename_const_go),
    ("add_param_go", edit_add_param_go),
    # insert_comment_go dropped: the from-go ingester strips native comments
    # during canonicalization, so the Nebula input contains no signal of the
    # inserted comment — we'd train the sidecar to hallucinate comments from
    # thin air. Dropped 2026-04-14 after the val eval showed 0% pass on this
    # edit type across 33 samples.
]


# ── Edit primitives (Python) ──────────────────────────────────────────────

FUNC_DEF_PY_RE = re.compile(r"^\s*def\s+([a-z_][a-z0-9_]*)\s*\(", re.MULTILINE)
ASSIGN_PY_RE = re.compile(r"^([a-z_][a-z0-9_]*)\s*=\s*[^=]", re.MULTILINE)
PY_BUILTIN_SKIP = {"init", "main", "test", "setup", "teardown", "run", "get", "set", "add"}


def edit_rename_func_py(src: str) -> Optional[str]:
    names = []
    for m in FUNC_DEF_PY_RE.finditer(src):
        n = m.group(1)
        if len(n) < 3 or n.startswith("_") or n in PY_BUILTIN_SKIP:
            continue
        names.append(n)
    if not names:
        return None
    name = names[0]
    new_name = f"{name}_fresh"
    pattern = rf"(?<![A-Za-z0-9_])({re.escape(name)})(?=\s*\()"
    modified = re.sub(pattern, lambda m: m.group(0).replace(name, new_name), src)
    return modified if modified != src else None


def edit_insert_comment_py(src: str) -> Optional[str]:
    m = re.search(r"^\s*def [^\n]+:\s*\n", src, re.MULTILINE)
    if not m:
        return None
    # Compute the indent of the body: find next non-blank line
    tail = src[m.end():]
    body_m = re.match(r"(\s*)", tail)
    indent = body_m.group(1) if body_m else "    "
    if "\n" in indent:
        indent = indent.split("\n")[-1]
    if not indent:
        indent = "    "
    insertion = f"{indent}# note: preserved by sidecar\n"
    modified = src[: m.end()] + insertion + src[m.end():]
    return modified if modified != src else None


PY_EDITS: list[tuple[str, Callable[[str], Optional[str]]]] = [
    ("rename_func_py", edit_rename_func_py),
    # insert_comment_py dropped for the same reason as insert_comment_go —
    # from-py strips comments. Val eval: 0/32 pass.
]


# ── Ingestion ─────────────────────────────────────────────────────────────

def ingest(src: str, lang: str, scratch: Path) -> Optional[str]:
    suffix = ".go" if lang == "go" else ".py"
    tmp = scratch / f"in{suffix}"
    tmp.write_text(src)
    cmd = [str(NEBULA), f"from-{lang}", str(tmp)]
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, cwd=str(scratch), timeout=30)
    except subprocess.TimeoutExpired:
        return None
    if out.returncode != 0:
        return None
    return out.stdout


def find_corpus() -> list[Path]:
    exts = ("*.go", "*.py")
    seen = set()
    files = []
    skip_parts = {"runs", "target", "__pycache__", ".venv", "raw_responses",
                  "sidecar_poc", "phase20_roundtrip"}
    for ext in exts:
        for p in REPO.rglob(ext):
            parts = set(p.parts)
            if skip_parts & parts:
                continue
            if p in seen:
                continue
            try:
                nlines = sum(1 for _ in p.read_text().splitlines())
            except Exception:
                continue
            if nlines < 10 or nlines > 200:
                continue
            seen.add(p)
            files.append(p)
    files.sort()
    return files


# ── Main ──────────────────────────────────────────────────────────────────

@dataclass
class Triple:
    file: str
    lang: str
    edit_type: str
    original: str
    modified_native: str
    modified_nebula: str


def build_triples(files: list[Path], scratch: Path, max_per_file: int = 3) -> list[Triple]:
    triples: list[Triple] = []
    stats = {"files_tried": 0, "edit_attempted": 0, "edit_applied": 0, "ingest_ok": 0, "ingest_fail": 0}
    for path in files:
        stats["files_tried"] += 1
        try:
            original = path.read_text()
        except Exception:
            continue
        lang = "go" if path.suffix == ".go" else "py"
        edits = GO_EDITS if lang == "go" else PY_EDITS

        applied_here = 0
        for edit_name, fn in edits:
            if applied_here >= max_per_file:
                break
            stats["edit_attempted"] += 1
            try:
                modified = fn(original)
            except Exception:
                modified = None
            if not modified or modified == original:
                continue
            stats["edit_applied"] += 1
            nebula = ingest(modified, lang, scratch)
            if not nebula:
                stats["ingest_fail"] += 1
                continue
            stats["ingest_ok"] += 1
            triples.append(Triple(
                file=str(path.relative_to(REPO)),
                lang=lang,
                edit_type=edit_name,
                original=original,
                modified_native=modified,
                modified_nebula=nebula,
            ))
            applied_here += 1
    return triples, stats


def to_chat_format(t: Triple) -> dict:
    return {
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": USER_TEMPLATE.format(
                lang=t.lang, original=t.original, nebula=t.modified_nebula,
            )},
            {"role": "assistant", "content": f"```{t.lang}\n{t.modified_native}```\n"},
        ],
        "_meta": {
            "file": t.file,
            "lang": t.lang,
            "edit_type": t.edit_type,
        },
    }


def main() -> None:
    max_files = int(os.environ.get("SYN_MAX_FILES", "0")) or None
    files = find_corpus()
    if max_files:
        files = files[:max_files]
    print(f"Corpus: {len(files)} files (Go + Python, 10–200 LoC)")

    scratch = OUT_DIR / "scratch"
    scratch.mkdir(exist_ok=True)

    triples, stats = build_triples(files, scratch)
    print(f"Stats: {stats}")
    print(f"Built {len(triples)} triples")

    # Shuffle deterministically, split 90/10 train/val
    random.shuffle(triples)
    split = int(len(triples) * 0.9)
    train, val = triples[:split], triples[split:]

    for name, subset in (("train", train), ("val", val)):
        out = OUT_DIR / f"sidecar_{name}.jsonl"
        with out.open("w") as fh:
            for t in subset:
                fh.write(json.dumps(to_chat_format(t)) + "\n")
        print(f"  {name}: {len(subset)} triples → {out}")

    # Quick edit-type histogram
    from collections import Counter
    hist_train = Counter(t.edit_type for t in train)
    hist_val = Counter(t.edit_type for t in val)
    print()
    print("Edit-type distribution:")
    for et in sorted(set(list(hist_train.keys()) + list(hist_val.keys()))):
        print(f"  {et:25s} train={hist_train.get(et,0):4d}  val={hist_val.get(et,0):4d}")


if __name__ == "__main__":
    main()
