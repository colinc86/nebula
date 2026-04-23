"""Iterate on eval-4 system prompt + tool descriptions without retraining.

prod-v2 dropped to 80% on eval-4 because two abstract prompts ("what would
this compile to?", "translate X to TS") got answered directly instead of
routing to the right tool. Neither prompt is wrong — it's a system-prompt
+ tool-description calibration issue on the deployment side.

This script tries a few variants and reports which cross the 90% bar.
No training cost.
"""
from __future__ import annotations
import json
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from eval_host_ft import TOOLCALL_EVAL_PROMPTS, TOOL_SCHEMAS_MIN

# Load API key.
for line in (HERE / "../../../.env").resolve().read_text().splitlines():
    if line.startswith("OPENAI_API_KEY="):
        os.environ["OPENAI_API_KEY"] = line.split("=", 1)[1].strip().strip('"').strip("'")
        break

from openai import OpenAI
client = OpenAI()
MODEL = "ft:gpt-4.1-2025-04-14:personal:nebulahost-prod-v2:DWvnbtaT"


# ── Variants ──────────────────────────────────────────────────────────

SYSTEM_BASELINE = (
    "You are a coding assistant that emits Nebula IL.\n\nNebula is a compact, "
    "language-agnostic intermediate representation. Always respond in Nebula "
    "when producing code. When you invoke tools that read or write source "
    "files, pass Nebula-form code, not native source. The host runtime "
    "translates Nebula to the target native language (Python, Go, JavaScript, "
    "TypeScript, Swift, Kotlin, Rust, C) on its side.\n\nWhen the user asks a "
    "general coding question that does not involve producing code for their "
    "project (e.g. conceptual explanations, debugging discussion, terminology), "
    "answer as you would normally."
)

SYSTEM_ROUTING = (
    SYSTEM_BASELINE
    + "\n\nTool-routing rules — ALWAYS use the matching tool when the user "
    "intent maps to one:\n"
    "- User asks to translate/port/convert code between languages → call "
    "`translate_code`, don't output the translation yourself.\n"
    "- User asks what code compiles to, or for a preview of Nebula → native → "
    "call `preview_translation`.\n"
    "- User asks to validate/check/parse Nebula syntax → call `check_nebula`.\n"
    "- User asks to read/view a source file → call `read_code` (or "
    "`read_native` if they explicitly want raw bytes).\n"
    "- User asks to search/grep/find in code → call `search_code`.\n"
    "- User asks to create/write a file → call `write_code`.\n"
    "- User asks to rename/edit/patch an existing file → call `patch_code`.\n"
    "Only answer in prose + Nebula directly when no tool matches the request."
)

TOOL_SCHEMAS_RICH = [
    {"type": "function", "function": {"name": "read_code",
        "description": "Read a source file, returning its contents in Nebula IL. USE WHEN the user asks to read, view, show, or inspect a source file.",
        "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
    {"type": "function", "function": {"name": "read_native",
        "description": "Read raw file bytes (non-source or binary files). USE WHEN the user explicitly wants raw bytes or the file is not source code.",
        "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
    {"type": "function", "function": {"name": "search_code",
        "description": "Grep/search code across files. USE WHEN the user asks to search, find, grep, or locate text in code.",
        "parameters": {"type": "object", "properties": {"query": {"type": "string"}, "path": {"type": "string"}, "glob": {"type": "string"}}, "required": ["query"]}}},
    {"type": "function", "function": {"name": "write_code",
        "description": "Write Nebula code to a new file. USE WHEN the user asks to create, write, or make a new file.",
        "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "code": {"type": "string"}}, "required": ["path", "code"]}}},
    {"type": "function", "function": {"name": "patch_code",
        "description": "Edit an existing file via old_str → new_str replacement. USE WHEN the user asks to rename, edit, patch, modify, or change existing code.",
        "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "old_str": {"type": "string"}, "new_str": {"type": "string"}, "nebula": {"type": "boolean"}}, "required": ["path", "old_str", "new_str"]}}},
    {"type": "function", "function": {"name": "translate_code",
        "description": "Translate Nebula or native code between languages. USE WHEN the user asks to translate, port, convert, or rewrite code in a different language.",
        "parameters": {"type": "object", "properties": {"code": {"type": "string"}, "source_language": {"type": "string"}, "target_language": {"type": "string"}}, "required": ["code", "source_language", "target_language"]}}},
    {"type": "function", "function": {"name": "check_nebula",
        "description": "Validate Nebula syntax, returning pass/fail with parse errors. USE WHEN the user asks to validate, check, parse, or verify Nebula code.",
        "parameters": {"type": "object", "properties": {"code": {"type": "string"}}, "required": ["code"]}}},
    {"type": "function", "function": {"name": "preview_translation",
        "description": "Dry-run the Nebula→native translation without writing to disk. USE WHEN the user asks what code compiles to, or for a preview of how Nebula maps to a native language.",
        "parameters": {"type": "object", "properties": {"code": {"type": "string"}, "target_language": {"type": "string"}}, "required": ["code", "target_language"]}}},
]


def run_variant(system_prompt: str, tool_schemas: list, temperature: float, name: str) -> tuple[float, list]:
    results = []
    passed = 0
    for p in TOOLCALL_EVAL_PROMPTS:
        resp = client.chat.completions.create(
            model=MODEL, max_tokens=400, temperature=temperature,
            tools=tool_schemas, tool_choice="auto",
            messages=[
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": p["prompt"]},
            ],
        )
        msg = resp.choices[0].message
        tool_calls = getattr(msg, "tool_calls", None) or []
        tool_name = None
        if tool_calls:
            tc = tool_calls[0]
            tool_name = getattr(tc.function, "name", None) if hasattr(tc, "function") else None
        allowed = {t["function"]["name"] for t in tool_schemas}
        ok = tool_name in allowed
        if ok:
            passed += 1
        results.append((p["prompt"][:50], tool_name or "(none)", p["expect_tool"], ok))
    rate = passed / len(TOOLCALL_EVAL_PROMPTS)
    print(f"\n=== {name}: {passed}/{len(TOOLCALL_EVAL_PROMPTS)} ({rate:.1%}) ===")
    for prompt, got, expected, ok in results:
        mark = "✓" if ok else "✗"
        print(f"  {mark} got={got:22} expected={expected:22} | {prompt}")
    return rate, results


def main():
    print(f"Tuning eval-4 against {MODEL}")
    variants = [
        ("baseline",          SYSTEM_BASELINE, TOOL_SCHEMAS_MIN,  0.2),
        ("routing_prompt",    SYSTEM_ROUTING,  TOOL_SCHEMAS_MIN,  0.2),
        ("rich_tool_desc",    SYSTEM_BASELINE, TOOL_SCHEMAS_RICH, 0.2),
        ("both_temp0",        SYSTEM_ROUTING,  TOOL_SCHEMAS_RICH, 0.0),
        ("both_temp02",       SYSTEM_ROUTING,  TOOL_SCHEMAS_RICH, 0.2),
    ]
    summary = []
    for name, sp, ts, t in variants:
        rate, _ = run_variant(sp, ts, t, name)
        summary.append((name, rate))
    print("\n\n═══ SUMMARY ═══")
    for name, rate in summary:
        print(f"  {rate:6.1%}  {name}")


if __name__ == "__main__":
    main()
