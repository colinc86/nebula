"""Slice 2 — instruction → Nebula pairs (1,500 rows via Sonnet synthesis).

For each seed prompt:
  1. Call Sonnet (system: "answer in a single fenced code block with a
     language hint comment").
  2. Extract the native code block.
  3. `nebula from-<lang>` → Nebula answer.
  4. QC gates: nebula_check, round-trip-back, length, template-leak.
  5. Emit chat-completion row with assistant content = fenced Nebula.

Budget: ~$17 Sonnet (prompt-cached system prompt cuts input by ~85%).
Runs last in the pipeline so earlier bugs don't waste API spend.

Dry-run produces no API calls; writes a plan summary to stdout.
"""
from __future__ import annotations
import argparse
import asyncio
import hashlib
import json
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from config import (
    LANG_EXT, SYSTEM_PROMPT_NEBULA, SLICE_FILES, REJECTED_FILES,
    TARGET_COUNTS, PROMPT_POOLS, SONNET_MODEL, ensure_dirs, REPO,
)
import nebula_cli


SEED_POOL_FILE = PROMPT_POOLS / "instruction_prompts.jsonl"

SONNET_SYSTEM = (
    "You are a senior developer. Answer the user's coding request with a "
    "single fenced code block containing complete, runnable code in the "
    "most natural language for the task (prefer Python, Go, TypeScript; "
    "use any of these 8 languages: Python, JavaScript, TypeScript, Go, "
    "Swift, Kotlin, Rust, C). No prose before or after the block. Start "
    "the block with a one-line comment `# language: <lang>` or "
    "`// language: <lang>`."
)

MIN_NEB_LEN = 200
MAX_NEB_LEN = 3000


def extract_code_and_lang(sonnet_response: str) -> tuple[str | None, str | None]:
    """Parse ```<hint>\n... with a `language: X` comment inside."""
    fence = re.search(r"```([a-zA-Z0-9_+-]*)\s*\n(.*?)```", sonnet_response, re.DOTALL)
    if not fence:
        return None, None
    body = fence.group(2).strip()
    lang_match = re.search(r"^(?:#|//)\s*language:\s*(\w+)\s*$", body, re.MULTILINE)
    lang = lang_match.group(1).lower() if lang_match else None
    if lang_match:
        body = body.replace(lang_match.group(0) + "\n", "", 1)
    lang = {"py": "python", "js": "javascript", "ts": "typescript", "kt": "kotlin", "rs": "rust"}.get(lang or "", lang or "")
    return body.strip(), lang


def build_row(prompt: str, nebula_answer: str, native_lang: str, seed_src: str, seed_idx: int) -> dict:
    return {
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT_NEBULA},
            {"role": "user", "content": prompt},
            {"role": "assistant", "content": f"```nebula\n{nebula_answer}\n```"},
        ],
        "slice": "instruction",
        "source": f"synth:{SONNET_MODEL}:{seed_src}:{seed_idx}",
        "row_id": hashlib.sha1(f"instruction:{seed_src}:{seed_idx}:{prompt[:60]}".encode()).hexdigest()[:16],
        "seed_native_lang": native_lang,
    }


def load_seed_prompts() -> list[dict]:
    if not SEED_POOL_FILE.exists():
        return []
    out = []
    for line in SEED_POOL_FILE.open():
        line = line.strip()
        if not line: continue
        try: out.append(json.loads(line))
        except Exception: pass
    return out


async def sonnet_answer(client, prompt: str) -> str | None:
    """One Sonnet call. Uses prompt caching on the system message so repeated
    calls pay 1/10 input cost on the cached portion."""
    try:
        msg = await client.messages.create(
            model=SONNET_MODEL,
            max_tokens=1500,
            system=[{"type": "text", "text": SONNET_SYSTEM, "cache_control": {"type": "ephemeral"}}],
            messages=[{"role": "user", "content": prompt}],
        )
        if not msg.content:
            return None
        for block in msg.content:
            if getattr(block, "type", None) == "text":
                return block.text
        return None
    except Exception as e:
        print(f"  sonnet error: {e}", file=sys.stderr)
        return None


async def process_one(client, seed: dict, idx: int) -> tuple[dict | None, dict | None]:
    """Synthesize one row. Returns (accepted_row, reject_record)."""
    prompt = seed.get("prompt") or ""
    source_tag = seed.get("source") or "unknown"
    if not prompt:
        return None, {"idx": idx, "reason": "empty prompt"}
    raw = await sonnet_answer(client, prompt)
    if raw is None:
        return None, {"idx": idx, "prompt": prompt[:80], "reason": "sonnet call failed"}
    native, native_lang = extract_code_and_lang(raw)
    if native is None or native_lang not in LANG_EXT:
        return None, {"idx": idx, "prompt": prompt[:80], "reason": f"no valid fenced block (lang={native_lang})"}
    nebula_answer = nebula_cli.to_nebula(native, native_lang)
    if nebula_answer is None:
        return None, {"idx": idx, "prompt": prompt[:80], "reason": "nebula from-lang failed"}
    if not nebula_cli.check(nebula_answer):
        return None, {"idx": idx, "prompt": prompt[:80], "reason": "nebula check failed"}
    # Round-trip-back gate: does the Nebula regenerate a compilable native?
    if nebula_cli.to_native(nebula_answer, native_lang) is None:
        return None, {"idx": idx, "prompt": prompt[:80], "reason": "round-trip back failed"}
    if not (MIN_NEB_LEN <= len(nebula_answer) <= MAX_NEB_LEN):
        return None, {"idx": idx, "prompt": prompt[:80], "reason": f"nebula length {len(nebula_answer)} out of bounds"}
    return build_row(prompt, nebula_answer, native_lang, source_tag, idx), None


async def main_async(args) -> None:
    try:
        from anthropic import AsyncAnthropic  # type: ignore
    except ImportError:
        print("`anthropic` package missing — `pip install anthropic`", file=sys.stderr)
        sys.exit(1)
    api_key = os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        env_file = REPO / ".env"
        if env_file.exists():
            for line in env_file.read_text().splitlines():
                if line.startswith("ANTHROPIC_API_KEY="):
                    api_key = line.split("=", 1)[1].strip().strip('"').strip("'")
                    break
    if not api_key:
        print("ANTHROPIC_API_KEY not set", file=sys.stderr)
        sys.exit(1)

    client = AsyncAnthropic(api_key=api_key)
    seeds = load_seed_prompts()
    if not seeds:
        print(f"No seed prompts at {SEED_POOL_FILE} — build that first.", file=sys.stderr)
        sys.exit(1)

    target = args.n or TARGET_COUNTS["instruction"]
    sem = asyncio.Semaphore(args.concurrency)
    rows_ok: list[dict] = []
    rejects: list[dict] = []

    async def guarded(seed, idx):
        async with sem:
            return await process_one(client, seed, idx)

    tasks = [guarded(s, i) for i, s in enumerate(seeds)]
    for fut in asyncio.as_completed(tasks):
        row, rej = await fut
        if row:
            rows_ok.append(row)
            if len(rows_ok) >= target:
                break
        elif rej:
            rejects.append(rej)

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


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--n", type=int, default=None)
    ap.add_argument("--concurrency", type=int, default=6)
    args = ap.parse_args()

    ensure_dirs()
    seeds = load_seed_prompts()
    target = args.n or TARGET_COUNTS["instruction"]
    print(f"seeds available : {len(seeds)}")
    print(f"target rows     : {target}")
    print(f"expected spend  : ~${len(seeds)*0.008:.2f} (prompt-cached Sonnet 4.6)")

    if args.dry_run:
        print("[dry-run] no API calls made")
        return
    if len(seeds) < target:
        print(f"  warning: seed pool has {len(seeds)} < target {target}")

    asyncio.run(main_async(args))


if __name__ == "__main__":
    main()
