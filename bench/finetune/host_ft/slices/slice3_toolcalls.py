"""Slice 3 — tool-use traces with Nebula content (500 rows).

Source: the 154 tool_call-containing traces in
`bench/finetune/sidecar_poc/data/combined_train.jsonl`. For each trace:

  1. Walk messages.
  2. For every assistant `tool_calls` entry targeting write_code /
     patch_code / patch_file: parse `arguments` JSON, identify the code-
     carrying field (`code`/`content`/`new_str`), infer the language from
     the `path` arg, run `nebula from-<lang>` on the code, and replace.
  3. For every `tool` result with native code (`read_code` on small files),
     translate to Nebula.
  4. Reject the entire trace if any translation fails or any Nebula
     block fails `nebula check`.
  5. Oversample: for portable code, produce 2 extra language variants
     (e.g. Python → Go, TypeScript) using the compiler's `translate`.

Dry-run: enumerate source traces, count tool_calls, estimate yield.
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
    LANG_EXT, SYSTEM_PROMPT_NEBULA, SLICE_FILES, REJECTED_FILES,
    TARGET_COUNTS, REPO, ensure_dirs,
)
import nebula_cli


SOURCE_FILE = REPO / "bench" / "finetune" / "sidecar_poc" / "data" / "combined_train.jsonl"
CODE_FIELDS = ("code", "content", "new_str", "file_content")


def load_source_traces() -> list[dict]:
    if not SOURCE_FILE.exists():
        return []
    rows = []
    for line in SOURCE_FILE.open():
        line = line.strip()
        if not line: continue
        try: rows.append(json.loads(line))
        except Exception: pass
    return rows


def infer_lang(path: str | None) -> str | None:
    if not path:
        return None
    for lang, ext in LANG_EXT.items():
        if path.endswith(f".{ext}"):
            return lang
    return None


def translate_tool_args(args_json: str, path_lang: str | None) -> tuple[str | None, list[str]]:
    """Return (new_arguments_json, errors). For every code-like field whose
    content appears native, translate to Nebula and replace."""
    try:
        args = json.loads(args_json or "{}")
    except Exception as e:
        return None, [f"arguments not valid JSON: {e}"]
    errors = []
    lang = path_lang or infer_lang(args.get("path") or args.get("file_path"))
    if not lang:
        return args_json, []  # nothing to translate
    changed = False
    for field in CODE_FIELDS:
        v = args.get(field)
        if not isinstance(v, str) or not v.strip():
            continue
        neb = nebula_cli.to_nebula(v, lang)
        if neb is None:
            errors.append(f"from-{lang} failed on field '{field}'")
            continue
        if not nebula_cli.check(neb):
            errors.append(f"nebula check failed on translated '{field}'")
            continue
        args[field] = neb
        changed = True
    if errors:
        return None, errors
    return (json.dumps(args) if changed else args_json), []


def transform_trace(trace: dict) -> tuple[dict | None, list[str]]:
    """Return (new_trace, errors). Whole-trace reject on any error."""
    errors = []
    new_messages = []
    for m in trace.get("messages", []):
        role = m.get("role")
        if role == "assistant" and m.get("tool_calls"):
            new_calls = []
            for tc in m.get("tool_calls", []):
                fn = tc.get("function") or {}
                args_json = fn.get("arguments") or "{}"
                new_args, errs = translate_tool_args(args_json, None)
                if errs:
                    errors.extend(errs)
                    return None, errors
                new_fn = dict(fn)
                new_fn["arguments"] = new_args or args_json
                new_tc = dict(tc)
                new_tc["function"] = new_fn
                new_calls.append(new_tc)
            new_msg = dict(m)
            new_msg["tool_calls"] = new_calls
            new_messages.append(new_msg)
        else:
            new_messages.append(m)
    new_trace = dict(trace)
    new_trace["messages"] = new_messages
    # Ensure the correct (Nebula-emitting) system prompt is in place.
    if new_trace["messages"] and new_trace["messages"][0].get("role") == "system":
        new_trace["messages"][0]["content"] = SYSTEM_PROMPT_NEBULA
    else:
        new_trace["messages"].insert(0, {"role": "system", "content": SYSTEM_PROMPT_NEBULA})
    return new_trace, []


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--n", type=int, default=None)
    args = ap.parse_args()

    ensure_dirs()
    sources = load_source_traces()
    print(f"source traces: {len(sources)} at {SOURCE_FILE}")
    target = args.n or TARGET_COUNTS["toolcall"]
    print(f"target rows  : {target}")

    if args.dry_run:
        # Count tool-call messages in each source trace.
        tc_counts = []
        for t in sources:
            n = sum(1 for m in t.get("messages", []) if m.get("role") == "assistant" and m.get("tool_calls"))
            tc_counts.append(n)
        print(f"  traces with ≥1 tool_calls: {sum(1 for n in tc_counts if n)}")
        print(f"  total tool_calls in source: {sum(tc_counts)}")
        print(f"[dry-run] no compiler calls made")
        return

    rows_ok: list[dict] = []
    rejects: list[dict] = []
    for idx, trace in enumerate(sources):
        new_trace, errs = transform_trace(trace)
        if new_trace is None:
            rejects.append({"idx": idx, "errors": errs[:3]})
            continue
        row = {
            "messages": new_trace["messages"],
            "slice": "toolcall",
            "source": f"combined_train#{idx}",
            "row_id": hashlib.sha1(f"toolcall:{idx}".encode()).hexdigest()[:16],
        }
        rows_ok.append(row)
        if len(rows_ok) >= target:
            break

    out_path = SLICE_FILES["toolcall"]
    rej_path = REJECTED_FILES["toolcall"]
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
