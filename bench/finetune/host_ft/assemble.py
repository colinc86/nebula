"""Merge the four slice files into the final train/val/held-out JSONL.

Strips provenance fields from the uploaded splits (OpenAI rejects unknown
top-level keys) but retains them in `*.enriched.jsonl` for audit. Writes a
`manifest.json` with counts, hashes, and per-slice QC rates.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import subprocess
from collections import Counter
from pathlib import Path

from config import (
    SLICE_FILES, FINAL, TARGET_COUNTS, HELD_OUT_TOTAL, BUILD, ensure_dirs,
)
from qc import row_id


KEEP_KEYS = {"messages", "tools", "parallel_tool_calls"}  # OpenAI schema

# OpenAI rejects training data containing its reserved tokenizer special
# tokens (would confuse the fine-tune / create prompt-injection risk).
OPENAI_RESERVED_TOKENS = [
    "<|endoftext|>", "<|im_start|>", "<|im_end|>",
    "<|endofprompt|>", "<|fim_prefix|>", "<|fim_middle|>", "<|fim_suffix|>",
]


def strip_to_openai(row: dict) -> dict:
    return {k: v for k, v in row.items() if k in KEEP_KEYS}


def row_has_reserved_token(row: dict) -> bool:
    """True if any message content in the row contains an OpenAI special token."""
    for m in row.get("messages", []):
        c = m.get("content")
        if isinstance(c, str):
            if any(t in c for t in OPENAI_RESERVED_TOKENS):
                return True
        elif isinstance(c, list):
            for part in c:
                if isinstance(part, dict):
                    txt = part.get("text", "")
                    if any(t in txt for t in OPENAI_RESERVED_TOKENS):
                        return True
    return False


def row_ends_with_assistant(row: dict) -> bool:
    """OpenAI requires the final message to be role=assistant."""
    msgs = row.get("messages", [])
    return bool(msgs) and msgs[-1].get("role") == "assistant"


def read_jsonl(p: Path) -> list[dict]:
    if not p.exists():
        return []
    out = []
    for line in p.open():
        line = line.strip()
        if not line:
            continue
        out.append(json.loads(line))
    return out


def write_jsonl(p: Path, rows: list[dict]) -> None:
    p.parent.mkdir(parents=True, exist_ok=True)
    with p.open("w") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")


def sha256_of_file(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(64 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def git_commit() -> str:
    try:
        r = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            capture_output=True, text=True, cwd=str(Path(__file__).parent),
            timeout=3,
        )
        return r.stdout.strip() or "unknown"
    except Exception:
        return "unknown"


def stratified_split(rows: list[dict], held_out_n: int, val_ratio: float = 0.1) -> tuple[list[dict], list[dict], list[dict]]:
    """Stratify by `slice` field across held_out, val, train.

    - Held-out: `held_out_n` rows total, proportional to slice sizes.
    - Remaining → deterministic val/train via `hash(row_id) % 10 < 1`.
    """
    by_slice: dict[str, list[dict]] = {}
    for r in rows:
        by_slice.setdefault(r.get("slice", "unknown"), []).append(r)

    # Proportional held-out counts, sorted for determinism.
    total = sum(len(v) for v in by_slice.values())
    held_out: list[dict] = []
    remaining: list[dict] = []
    for slc, items in sorted(by_slice.items()):
        # Deterministic shuffle: sort by row_id hash.
        items = sorted(items, key=lambda r: hashlib.sha1(
            row_id(r).encode("utf-8")
        ).hexdigest())
        slice_ho = round(len(items) / total * held_out_n) if total else 0
        held_out.extend(items[:slice_ho])
        remaining.extend(items[slice_ho:])

    # Deterministic val/train split on the remainder.
    train, val = [], []
    for r in remaining:
        h = int(hashlib.sha1(row_id(r).encode("utf-8")).hexdigest(), 16)
        (val if h % 10 == 0 else train).append(r)
    return train, val, held_out


def slice_proportion_check(train: list[dict], val: list[dict], tol_pp: float = 2.0) -> list[str]:
    """Return list of warnings if train/val slice proportions drift more
    than `tol_pp` percentage points."""
    total_train = len(train) or 1
    total_val = len(val) or 1
    warnings = []
    slices = set(r.get("slice", "unknown") for r in (train + val))
    for slc in sorted(slices):
        p_train = sum(1 for r in train if r.get("slice") == slc) / total_train * 100
        p_val = sum(1 for r in val if r.get("slice") == slc) / total_val * 100
        delta = abs(p_train - p_val)
        if delta > tol_pp:
            warnings.append(
                f"slice '{slc}': train={p_train:.1f}% val={p_val:.1f}% "
                f"(Δ={delta:.1f}pp > {tol_pp}pp)"
            )
    return warnings


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--strict-counts", action="store_true",
                    help="Error if any slice is more than ±10 rows off target.")
    args = ap.parse_args()

    ensure_dirs()

    # Load slices.
    all_rows: list[dict] = []
    per_slice_counts: Counter = Counter()
    per_slice_qc: dict[str, dict] = {}
    dropped_reserved = 0
    dropped_non_assistant_end = 0
    for name, p in SLICE_FILES.items():
        rows = read_jsonl(p)
        # Filter OpenAI-reserved tokens (would fail fine-tune validation).
        before = len(rows)
        rows = [r for r in rows if not row_has_reserved_token(r)]
        dropped_reserved += before - len(rows)
        # Filter rows not ending with assistant (OpenAI fine-tune requirement).
        before = len(rows)
        rows = [r for r in rows if row_ends_with_assistant(r)]
        dropped_non_assistant_end += before - len(rows)
        per_slice_counts[name] = len(rows)
        # Ensure each row has a `slice` field.
        for r in rows:
            r.setdefault("slice", name)
        all_rows.extend(rows)
    if dropped_reserved:
        print(f"Dropped {dropped_reserved} rows with OpenAI-reserved tokens (<|endoftext|>, etc.)")
    if dropped_non_assistant_end:
        print(f"Dropped {dropped_non_assistant_end} rows not ending with role=assistant")
        rej_path = BUILD / f"{p.stem}.rejected.jsonl"
        rej_n = len(read_jsonl(rej_path))
        per_slice_qc[name] = {
            "accepted": len(rows),
            "rejected": rej_n,
            "rate": (len(rows) / (len(rows) + rej_n)) if (len(rows) + rej_n) else None,
        }

    print(f"Loaded {len(all_rows)} rows:")
    for name, count in per_slice_counts.items():
        tgt = TARGET_COUNTS.get(name, 0)
        flag = ""
        if args.strict_counts and abs(count - tgt) > 10:
            flag = f" ⚠ off target ({tgt} ±10)"
        print(f"  {name}: {count}/{tgt}{flag}")

    if args.strict_counts:
        for name, count in per_slice_counts.items():
            tgt = TARGET_COUNTS.get(name, 0)
            if abs(count - tgt) > 10:
                raise SystemExit(f"strict: slice {name} count {count} off target {tgt}")

    # Stratified split.
    train, val, held_out = stratified_split(all_rows, HELD_OUT_TOTAL)
    print(f"Split → train={len(train)} val={len(val)} held_out={len(held_out)}")

    # Proportion sanity check.
    for w in slice_proportion_check(train, val):
        print(f"  warning: {w}")

    if args.dry_run:
        print("(--dry-run) skipping writes")
        return

    # Write enriched + stripped variants.
    write_jsonl(FINAL["enriched_train"], train)
    write_jsonl(FINAL["enriched_val"], val)
    write_jsonl(FINAL["enriched_held_out"], held_out)
    write_jsonl(FINAL["train"], [strip_to_openai(r) for r in train])
    write_jsonl(FINAL["val"], [strip_to_openai(r) for r in val])
    write_jsonl(FINAL["held_out"], [strip_to_openai(r) for r in held_out])

    manifest = {
        "slices": per_slice_counts,
        "qc": per_slice_qc,
        "splits": {
            "train": len(train),
            "val": len(val),
            "held_out": len(held_out),
        },
        "file_hashes": {
            k: sha256_of_file(FINAL[k]) for k in ("train", "val", "held_out")
        },
        "generator_commit": git_commit(),
    }
    FINAL["manifest"].write_text(json.dumps(manifest, indent=2))
    print(f"Wrote manifest → {FINAL['manifest']}")


if __name__ == "__main__":
    main()
