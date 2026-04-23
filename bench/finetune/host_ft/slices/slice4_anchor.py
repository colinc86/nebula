"""Slice 4 — behavior anchor (200 rows).

Prompts: `anchor_prompts.jsonl` (250 seeds; 50 headroom for dedup).
Targets: stock `gpt-4.1-mini-2025-07-18` responses (no Nebula system
prompt). Teaches the model: "under the plain coding-assistant system
prompt, answer as you would normally — do not reflexively emit Nebula."

Budget: ~$0.06. Runs after slice 4 prompt pool is built.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from config import (
    SYSTEM_PROMPT_ANCHOR, SLICE_FILES, REJECTED_FILES, TARGET_COUNTS,
    PROMPT_POOLS, STOCK_MINI, ensure_dirs, REPO,
)


ANCHOR_POOL = PROMPT_POOLS / "anchor_prompts.jsonl"


def load_prompts() -> list[dict]:
    if not ANCHOR_POOL.exists():
        return []
    out = []
    for line in ANCHOR_POOL.open():
        line = line.strip()
        if not line: continue
        try: out.append(json.loads(line))
        except Exception: pass
    return out


def build_row(prompt: str, response: str, seed_src: str, idx: int) -> dict:
    return {
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT_ANCHOR},
            {"role": "user", "content": prompt},
            {"role": "assistant", "content": response},
        ],
        "slice": "anchor",
        "source": f"stock:{STOCK_MINI}:{seed_src}:{idx}",
        "row_id": hashlib.sha1(f"anchor:{seed_src}:{idx}:{prompt[:40]}".encode()).hexdigest()[:16],
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--n", type=int, default=None)
    args = ap.parse_args()

    ensure_dirs()
    prompts = load_prompts()
    target = args.n or TARGET_COUNTS["anchor"]
    print(f"seed prompts : {len(prompts)}")
    print(f"target rows  : {target}")
    print(f"model        : {STOCK_MINI}")

    if args.dry_run:
        print("[dry-run] no OpenAI calls made")
        return

    try:
        from openai import OpenAI  # type: ignore
    except ImportError:
        print("`openai` package missing — `pip install openai`", file=sys.stderr)
        sys.exit(1)

    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        env_file = REPO / ".env"
        if env_file.exists():
            for line in env_file.read_text().splitlines():
                if line.startswith("OPENAI_API_KEY="):
                    api_key = line.split("=", 1)[1].strip().strip('"').strip("'")
                    break
    if not api_key:
        print("OPENAI_API_KEY not set", file=sys.stderr)
        sys.exit(1)

    client = OpenAI(api_key=api_key)
    rows_ok: list[dict] = []
    rejects: list[dict] = []
    for idx, seed in enumerate(prompts):
        if len(rows_ok) >= target:
            break
        prompt = seed.get("prompt") or ""
        src = seed.get("source") or "unknown"
        if not prompt:
            continue
        try:
            resp = client.chat.completions.create(
                model=STOCK_MINI,
                max_tokens=1000,
                messages=[
                    {"role": "system", "content": SYSTEM_PROMPT_ANCHOR},
                    {"role": "user", "content": prompt},
                ],
            )
            content = resp.choices[0].message.content or ""
        except Exception as e:
            rejects.append({"idx": idx, "reason": f"openai error: {e}"})
            continue
        if not content.strip():
            rejects.append({"idx": idx, "reason": "empty response"})
            continue
        rows_ok.append(build_row(prompt, content.strip(), src, idx))

    out_path = SLICE_FILES["anchor"]
    rej_path = REJECTED_FILES["anchor"]
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
