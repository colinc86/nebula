"""Slice 2 DIRECT — build instruction→Nebula rows from curated pairs
(no Sonnet synthesis, $0).

Pulls (prompt, nebula_answer) tuples from the existing curated lists in
`bench/finetune/curated_pairs.py` + `curated_pairs_extra.py`, validates
each Nebula answer via `nebula check`, and emits training rows.

This is a cheap foundation for slice 2. The expensive `slice2_instruction.py`
script adds ~1,000 more rows via Sonnet synthesis on top.

Run order: call this script first (no API, ~2s), then
`slice2_instruction.py` later for the rest. Both write to
`build/slice2_instruction.jsonl` — this one overwrites; the synthesis
script should append or dedupe (see its --resume flag).
"""
from __future__ import annotations
import argparse
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from config import (
    SYSTEM_PROMPT_NEBULA, SLICE_FILES, REJECTED_FILES,
    MIN_NEBULA_LEN, MAX_NEBULA_LEN, ensure_dirs, REPO,
)
import nebula_cli


def load_curated_pairs() -> list[tuple[str, str, str]]:
    """Return list of (prompt, nebula_answer, source_tag)."""
    sys.path.insert(0, str(REPO / "bench" / "finetune"))
    import curated_pairs as cp  # type: ignore
    import curated_pairs_extra as cpe  # type: ignore
    pairs: list[tuple[str, str, str]] = []
    for tup in cp.DRILLS:
        pairs.append((tup[0], tup[1], "curated_pairs.DRILLS"))
    for tup in cp.SHORTEST_PROMPTS:
        pairs.append((tup[0], tup[1], "curated_pairs.SHORTEST_PROMPTS"))
    for list_name in ("CONCURRENCY_DRILLS", "ERROR_DRILLS", "MATCH_DRILLS",
                      "STRUCT_DRILLS", "TEST_DRILLS"):
        lst = getattr(cpe, list_name, [])
        for tup in lst:
            pairs.append((tup[0], tup[1], f"curated_pairs_extra.{list_name}"))
    return pairs


def build_row(prompt: str, nebula: str, source: str, idx: int) -> dict:
    return {
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT_NEBULA},
            {"role": "user", "content": prompt},
            {"role": "assistant", "content": f"```nebula\n{nebula}\n```"},
        ],
        "slice": "instruction",
        "source": f"{source}:{idx}",
        "row_id": hashlib.sha1(f"instruction:{source}:{idx}:{prompt[:60]}".encode()).hexdigest()[:16],
        "seed_native_lang": None,
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    ensure_dirs()

    pairs = load_curated_pairs()
    print(f"Loaded {len(pairs)} curated pairs from curated_pairs + _extra.")

    if args.dry_run:
        print("[dry-run] no writes made")
        return

    rows_ok: list[dict] = []
    rejects: list[dict] = []
    seen_ids: set[str] = set()
    for idx, (prompt, nebula, source) in enumerate(pairs):
        # Length guard
        if not (MIN_NEBULA_LEN <= len(nebula) <= MAX_NEBULA_LEN):
            rejects.append({"idx": idx, "source": source, "reason": f"len {len(nebula)} out of bounds"})
            continue
        # Syntax guard
        if not nebula_cli.check(nebula):
            rejects.append({"idx": idx, "source": source, "reason": "nebula check failed", "nebula": nebula[:120]})
            continue
        row = build_row(prompt, nebula, source, idx)
        # Dedup on prompt
        pid = hashlib.sha1(prompt.encode()).hexdigest()
        if pid in seen_ids:
            continue
        seen_ids.add(pid)
        rows_ok.append(row)

    out_path = SLICE_FILES["instruction"]
    rej_path = REJECTED_FILES["instruction"]
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
