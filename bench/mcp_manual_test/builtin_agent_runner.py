#!/usr/bin/env python3
"""Minimal agent runner with generic file-editing tools (no Nebula MCP).

Exists to benchmark stock Qwen3-Coder-30B (or any tool-capable Ollama model) on
the same implement/refactor tasks `host_agent_runner.py` runs, but with a
generic file-tool scaffold instead of the Nebula MCP. The point is to measure
what the model's token cost looks like with ordinary `read_file` / `write_file`
/ `edit_file` / `list_files` / `bash` tools so we can put a matched A/B
comparison (builtins vs. Nebula MCP) in the benchmark results.

Tools exposed:
  read_file(path)
  write_file(path, content)
  edit_file(path, old_str, new_str, replace_all=false)
  list_files(pattern=None)
  bash(command)

Token accounting uses Ollama's `prompt_eval_count` (input) and `eval_count`
(output) per turn. Unlike Anthropic, Ollama doesn't surface cache_read vs
cache_create separately — we report raw input and output, summed across turns.
"""

import argparse
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path


SYSTEM_PROMPT = """You are an autonomous coding agent. You have access to these tools:
  - read_file(path): return the raw bytes of a file
  - write_file(path, content): overwrite a file completely
  - edit_file(path, old_str, new_str, replace_all): replace one (or all) occurrences of old_str with new_str in a file
  - list_files(pattern): list files in the working directory (optional glob pattern)
  - bash(command): run a shell command in the working directory, return combined stdout+stderr

Working principles:
- Call tools decisively. Don't narrate; call the tool and look at the result.
- Prefer edit_file over write_file for small changes. Use replace_all=true when renaming something that appears in many places.
- After making changes, run the verification command (go test, pytest, etc.) to confirm.
- BEFORE responding with your final summary, run the verification command one last time. A passing build is the only signal that you're done.
- When the task is fully complete and the verification passes, respond with a brief plain-text summary and stop calling tools."""


TOOLS_SCHEMA = [
    {
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read a file from the working directory and return its contents as text.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file, relative to the working directory"},
                },
                "required": ["path"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Overwrite a file with new contents. Creates the file if it does not exist.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"},
                },
                "required": ["path", "content"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "edit_file",
            "description": "Replace occurrences of old_str with new_str in a file. Fails if old_str is not unique and replace_all is false.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "old_str": {"type": "string", "description": "Exact substring to replace"},
                    "new_str": {"type": "string"},
                    "replace_all": {"type": "boolean", "description": "If true, replace every occurrence", "default": False},
                },
                "required": ["path", "old_str", "new_str"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "list_files",
            "description": "List files in the working directory. Optionally filter by a glob pattern.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Optional glob, e.g. '*.go' or '**/*.py'"},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "bash",
            "description": "Run a shell command in the working directory. Returns combined stdout+stderr.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {"type": "string"},
                },
                "required": ["command"],
            },
        },
    },
]


def tool_read_file(cwd, args):
    path = args.get("path", "")
    try:
        p = (Path(cwd) / path).resolve()
        if Path(cwd).resolve() not in p.parents and p != Path(cwd).resolve():
            return f"ERROR: path {path} outside working directory"
        return p.read_text()
    except FileNotFoundError:
        return f"ERROR: file not found: {path}"
    except Exception as e:
        return f"ERROR: {type(e).__name__}: {e}"


def tool_write_file(cwd, args):
    path = args.get("path", "")
    content = args.get("content", "")
    try:
        p = (Path(cwd) / path).resolve()
        if Path(cwd).resolve() not in p.parents and p != Path(cwd).resolve():
            return f"ERROR: path {path} outside working directory"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content)
        return f"Wrote {len(content)} bytes to {path}"
    except Exception as e:
        return f"ERROR: {type(e).__name__}: {e}"


def tool_edit_file(cwd, args):
    path = args.get("path", "")
    old_str = args.get("old_str", "")
    new_str = args.get("new_str", "")
    replace_all = bool(args.get("replace_all", False))
    try:
        p = (Path(cwd) / path).resolve()
        if Path(cwd).resolve() not in p.parents and p != Path(cwd).resolve():
            return f"ERROR: path {path} outside working directory"
        source = p.read_text()
        if not old_str:
            return "ERROR: old_str must be non-empty"
        occurrences = source.count(old_str)
        if occurrences == 0:
            return f"ERROR: old_str not found in {path}"
        if occurrences > 1 and not replace_all:
            return f"ERROR: old_str appears {occurrences} times in {path}; pass replace_all=true or pick a more specific old_str"
        new_source = source.replace(old_str, new_str) if replace_all else source.replace(old_str, new_str, 1)
        p.write_text(new_source)
        return f"Edited {path} ({occurrences} occurrence{'s' if occurrences != 1 else ''} replaced)"
    except FileNotFoundError:
        return f"ERROR: file not found: {path}"
    except Exception as e:
        return f"ERROR: {type(e).__name__}: {e}"


def tool_list_files(cwd, args):
    pattern = args.get("pattern")
    root = Path(cwd)
    try:
        if pattern:
            matches = sorted(str(p.relative_to(root)) for p in root.glob(pattern) if p.is_file())
        else:
            matches = sorted(
                str(p.relative_to(root)) for p in root.rglob("*")
                if p.is_file() and ".git" not in p.parts
            )
        if not matches:
            return "(no files match)"
        return "\n".join(matches)
    except Exception as e:
        return f"ERROR: {type(e).__name__}: {e}"


def tool_bash(cwd, args):
    cmd = args.get("command", "")
    if not cmd:
        return "ERROR: command must be non-empty"
    try:
        p = subprocess.run(cmd, shell=True, cwd=cwd, capture_output=True, text=True, timeout=120)
        out = (p.stdout + p.stderr)
        if not out:
            out = f"(exit {p.returncode}, no output)"
        return out[-6000:]
    except subprocess.TimeoutExpired:
        return "ERROR: command timed out after 120s"
    except Exception as e:
        return f"ERROR: {type(e).__name__}: {e}"


TOOL_DISPATCH = {
    "read_file": tool_read_file,
    "write_file": tool_write_file,
    "edit_file": tool_edit_file,
    "list_files": tool_list_files,
    "bash": tool_bash,
}


def ollama_chat(url, model, messages, tools, num_ctx, temperature):
    body = {
        "model": model,
        "messages": messages,
        "tools": tools,
        "stream": False,
        "options": {"num_ctx": num_ctx, "temperature": temperature},
    }
    req = urllib.request.Request(
        url.rstrip("/") + "/api/chat",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=600) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        err_body = ""
        try:
            err_body = e.read().decode(errors="replace")[:800]
        except Exception:
            pass
        raise RuntimeError(f"Ollama HTTP {e.code}: {err_body}")


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--model", required=True)
    p.add_argument("--ollama-url", default="http://192.168.68.94:11434")
    p.add_argument("--task-dir", required=True)
    p.add_argument("--prompt-file", help="File containing the user prompt")
    p.add_argument("--prompt", help="Inline prompt (alternative to --prompt-file)")
    p.add_argument("--verify", help="Verify command to run after the agent loop")
    p.add_argument("--output", required=True, help="JSONL trace output")
    p.add_argument("--max-turns", type=int, default=60)
    p.add_argument("--num-ctx", type=int, default=32768)
    p.add_argument("--temperature", type=float, default=0.1)
    args = p.parse_args()

    task_dir = str(Path(args.task_dir).resolve())
    if args.prompt_file:
        prompt = Path(args.prompt_file).read_text()
    elif args.prompt:
        prompt = args.prompt
    else:
        sys.exit("ERROR: pass --prompt or --prompt-file")

    print("=== Builtin-tools agent runner ===", flush=True)
    print(f"  model:       {args.model}")
    print(f"  ollama_url:  {args.ollama_url}")
    print(f"  task_dir:    {task_dir}")
    print(f"  output:      {args.output}")
    print(f"  max_turns:   {args.max_turns}")
    print(flush=True)

    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": prompt},
    ]
    trace = []
    stats = {
        "turns": 0,
        "tool_calls": 0,
        "tool_breakdown": {},
        "input_tokens": 0,
        "output_tokens": 0,
        "stopped": "max_turns",
    }
    t_start = time.time()

    for turn in range(args.max_turns):
        stats["turns"] = turn + 1
        try:
            resp = ollama_chat(args.ollama_url, args.model, messages, TOOLS_SCHEMA, args.num_ctx, args.temperature)
        except Exception as e:
            print(f"  ERROR calling ollama: {e}", flush=True)
            stats["stopped"] = f"ollama_error: {e}"
            break

        stats["input_tokens"] += resp.get("prompt_eval_count", 0) or 0
        stats["output_tokens"] += resp.get("eval_count", 0) or 0

        msg = resp.get("message") or {}
        content = msg.get("content") or ""
        raw_tool_calls = msg.get("tool_calls") or []

        norm_tcs = []
        for i, tc in enumerate(raw_tool_calls):
            fn = tc.get("function") or {}
            args_field = fn.get("arguments")
            if isinstance(args_field, str):
                try:
                    parsed_args = json.loads(args_field) if args_field else {}
                except Exception:
                    parsed_args = {}
            elif isinstance(args_field, dict):
                parsed_args = args_field
            else:
                parsed_args = {}
            norm_tcs.append({
                "id": tc.get("id") or f"call_{turn}_{i}",
                "name": fn.get("name", ""),
                "args": parsed_args,
            })

        assistant_out = {"role": "assistant", "content": content}
        if norm_tcs:
            # Ollama wants `arguments` as a dict in the request body, not a string.
            assistant_out["tool_calls"] = [
                {
                    "id": tc["id"],
                    "type": "function",
                    "function": {"name": tc["name"], "arguments": tc["args"]},
                }
                for tc in norm_tcs
            ]
        messages.append(assistant_out)

        turn_log = {
            "turn": turn + 1,
            "assistant_content": content[:500],
            "tool_calls": [{"name": tc["name"], "args": tc["args"]} for tc in norm_tcs],
            "input_tokens": resp.get("prompt_eval_count", 0) or 0,
            "output_tokens": resp.get("eval_count", 0) or 0,
        }

        if not norm_tcs:
            trace.append(turn_log)
            stats["stopped"] = "final_message"
            print(f"  turn {turn+1}: final message ({len(content)} chars)", flush=True)
            break

        results = []
        for tc in norm_tcs:
            name = tc["name"]
            stats["tool_calls"] += 1
            stats["tool_breakdown"][name] = stats["tool_breakdown"].get(name, 0) + 1
            fn = TOOL_DISPATCH.get(name)
            if fn is None:
                result = f"ERROR: unknown tool {name}"
            else:
                try:
                    result = fn(task_dir, tc["args"])
                except Exception as e:
                    result = f"ERROR: {type(e).__name__}: {e}"
            result = result[:8000]
            results.append({"name": name, "result_preview": result[:200]})
            messages.append({
                "role": "tool",
                "tool_call_id": tc["id"],
                "name": name,
                "content": result,
            })

        turn_log["tool_results"] = results
        trace.append(turn_log)
        print(f"  turn {turn+1}: {len(norm_tcs)} tool call(s) {[tc['name'] for tc in norm_tcs]}", flush=True)

    duration = time.time() - t_start

    verify_result = None
    if args.verify:
        try:
            vp = subprocess.run(args.verify, shell=True, cwd=task_dir, capture_output=True, text=True, timeout=180)
            verify_result = {"exit": vp.returncode, "tail": (vp.stdout + vp.stderr)[-800:]}
        except Exception as e:
            verify_result = {"exit": -1, "tail": f"exception: {e}"}

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        f.write(json.dumps({"type": "meta", "model": args.model, "task_dir": task_dir, "stats": stats, "duration": duration, "verify": verify_result}) + "\n")
        for t in trace:
            f.write(json.dumps({"type": "turn", **t}) + "\n")

    print()
    print("=== Summary ===")
    print(f"  stopped:        {stats['stopped']}")
    print(f"  turns:          {stats['turns']}")
    print(f"  tool_calls:     {stats['tool_calls']}  {stats['tool_breakdown']}")
    print(f"  input_tokens:   {stats['input_tokens']}")
    print(f"  output_tokens:  {stats['output_tokens']}")
    print(f"  duration:       {duration:.1f}s")
    if verify_result is not None:
        print(f"  verify:         {'PASS' if verify_result['exit'] == 0 else 'FAIL'} ({args.verify})")
        print(f"  verify_tail:    {verify_result['tail'].strip()[:200]}")
    print()
    print(f"Log written to {out_path}")


if __name__ == "__main__":
    main()
