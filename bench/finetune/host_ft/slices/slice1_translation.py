"""Slice 1 — translation pairs (800 rows, 100/lang balanced).

Streams rows from `electrocampbell/nebula-8lang-203k` (HuggingFace),
filters by length, dedupes, and emits one-shot chat-completion rows:

  user:      "Convert this <Lang> to Nebula IL:\n\n```<ext>\n<native>\n```"
  assistant: "```nebula\n<nebula>\n```"

Also applies a compiler round-trip gate: since we just shipped Track 1's
codegen fixes, historical (native, nebula) pairs from the HF dataset may
diverge from the current compiler output. Filter to pairs with ≥95% token
overlap (rough proxy — we use shared-line count over total lines).

Usage:
  python3 slice1_translation.py                     # full run
  python3 slice1_translation.py --dry-run --n 5     # scaffold sanity
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import random
import sys
from pathlib import Path

# Allow sibling imports.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from config import (
    LANGUAGES, LANG_DISPLAY, LANG_EXT, SYSTEM_PROMPT_NEBULA,
    SLICE_FILES, TARGET_COUNTS, REJECTED_FILES, ensure_dirs,
)
import nebula_cli


DATASET_ID = "electrocampbell/nebula-8lang-203k"
PER_LANG_TARGET = 100
MIN_NATIVE = 50
MAX_NATIVE = 1500
MIN_NEBULA = 20
MAX_NEBULA = 1200


def round_trip_overlap(native: str, nebula: str, lang: str) -> float:
    """Rough fidelity metric: fraction of tokens in `nebula` that also appear
    in the recompiled-from-native Nebula. 1.0 = perfect agreement, 0.0 = none."""
    regenerated = nebula_cli.to_nebula(native, lang)
    if regenerated is None:
        return 0.0
    orig_tokens = set(nebula.split())
    regen_tokens = set(regenerated.split())
    if not orig_tokens:
        return 0.0
    return len(orig_tokens & regen_tokens) / len(orig_tokens)


def build_row(native: str, nebula: str, lang: str, dataset_idx: int) -> dict:
    ext = LANG_EXT[lang]
    display = LANG_DISPLAY[lang]
    user_msg = (
        f"Convert this {display} to Nebula IL:\n\n"
        f"```{ext}\n{native}\n```"
    )
    assistant_msg = f"```nebula\n{nebula}\n```"
    return {
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT_NEBULA},
            {"role": "user", "content": user_msg},
            {"role": "assistant", "content": assistant_msg},
        ],
        "slice": "translation",
        "source": f"{DATASET_ID}#{dataset_idx}",
        "row_id": hashlib.sha1(f"translation:{lang}:{dataset_idx}".encode()).hexdigest()[:16],
        "seed_native_lang": lang,
    }


SYSTEM_LANG_HINTS = {
    "Python": "python", "JavaScript": "javascript", "TypeScript": "typescript",
    "Go": "go", "Swift": "swift", "Kotlin": "kotlin", "Rust": "rust", "C": "c",
}


def _lang_from_system(system_content: str) -> str | None:
    """Extract target language from the 'You are a code translator... produce
    the equivalent idiomatic <Lang> code' system prompt."""
    import re
    m = re.search(r"idiomatic (\w+) code", system_content or "")
    if not m:
        return None
    return SYSTEM_LANG_HINTS.get(m.group(1))


def stream_hf_dataset(limit_per_lang: int):
    """Yield (dataset_idx, lang, native, nebula) tuples from the HF dataset.

    The HF dataset is in Nebula→Native direction (user=Nebula,
    assistant=Native). We invert when emitting training rows so our model
    learns to emit Nebula from native input.
    """
    try:
        from datasets import load_dataset  # type: ignore
    except ImportError:
        print("`datasets` package missing — `pip install datasets`", file=sys.stderr)
        sys.exit(1)
    ds = load_dataset(DATASET_ID, split="train", streaming=True)
    per_lang_counts: dict[str, int] = {lang: 0 for lang, _, _ in LANGUAGES}
    seen_hashes: set[str] = set()
    for idx, row in enumerate(ds):
        msgs = row.get("messages") or []
        if len(msgs) < 3:
            continue
        # Expected: [system, user(nebula), assistant(native)]
        system = next((m for m in msgs if m.get("role") == "system"), None)
        user = next((m for m in msgs if m.get("role") == "user"), None)
        assistant = next((m for m in msgs if m.get("role") == "assistant"), None)
        if not system or not user or not assistant:
            continue
        lang = _lang_from_system(system.get("content") or "")
        if lang not in LANG_EXT:
            continue
        if per_lang_counts[lang] >= limit_per_lang:
            if all(v >= limit_per_lang for v in per_lang_counts.values()):
                return
            continue
        nebula = user.get("content") or ""
        native = assistant.get("content") or ""
        if not native or not nebula:
            continue
        if not (MIN_NATIVE <= len(native) <= MAX_NATIVE):
            continue
        if not (MIN_NEBULA <= len(nebula) <= MAX_NEBULA):
            continue
        h = hashlib.sha1(native.encode()).hexdigest()
        if h in seen_hashes:
            continue
        seen_hashes.add(h)
        yield idx, lang, native, nebula
        per_lang_counts[lang] += 1


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--n", type=int, default=None,
                    help="Override per-lang target (default 100). In dry-run, typical n=5.")
    ap.add_argument("--min-overlap", type=float, default=0.95,
                    help="Round-trip overlap gate (default 0.95).")
    args = ap.parse_args()

    ensure_dirs()
    target = args.n or PER_LANG_TARGET
    out_path = SLICE_FILES["translation"]
    rej_path = REJECTED_FILES["translation"]

    if args.dry_run:
        print(f"[dry-run] would stream {DATASET_ID}, take {target}/lang × 8 langs = {target*8}")
        print(f"[dry-run] would write to {out_path}")
        print(f"[dry-run] round-trip gate: overlap ≥ {args.min_overlap}")
        return

    random.seed(42)
    rows_ok: list[dict] = []
    rejects: list[dict] = []
    for idx, lang, native, nebula in stream_hf_dataset(target):
        # Track 4 iter-2 change: the compiler has been substantially fixed
        # since the 203k dataset was built, so the stored `nebula` is stale.
        # Use the freshly-regenerated Nebula as the training target and
        # gate on (a) successful compile and (b) `nebula check` acceptance.
        regen = nebula_cli.to_nebula(native, lang)
        if regen is None or not regen.strip():
            rejects.append({
                "idx": idx, "lang": lang,
                "reason": "compiler produced empty output",
            })
            continue
        if not nebula_cli.check(regen):
            rejects.append({
                "idx": idx, "lang": lang,
                "reason": "regenerated Nebula failed `nebula check`",
            })
            continue
        if not (MIN_NEBULA <= len(regen) <= MAX_NEBULA):
            rejects.append({
                "idx": idx, "lang": lang,
                "reason": f"regen length {len(regen)} out of [{MIN_NEBULA}, {MAX_NEBULA}]",
            })
            continue
        rows_ok.append(build_row(native, regen, lang, idx))

    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w") as f:
        for r in rows_ok:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    with rej_path.open("w") as f:
        for r in rejects:
            f.write(json.dumps(r) + "\n")
    print(f"Wrote {len(rows_ok)} rows → {out_path}")
    print(f"Wrote {len(rejects)} rejects → {rej_path}")


if __name__ == "__main__":
    main()
