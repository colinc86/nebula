#!/usr/bin/env python3
"""Distillation runner — runs Sonnet through Nebula MCP on each task and
captures the agent traces as fine-tuning data.

For every task in tasks/:
    1. Copy seed/ to a fresh trial directory
    2. Run `claude -p --model sonnet --mcp-config ... --output-format stream-json`
       with the task's prompt against that trial dir
    3. Run the verify command in the trial dir
    4. If verify passes (or for explain tasks, the final message contains the
       expected keywords), convert the stream into Qwen3 chat-template format
       and append to the output JSONL
    5. If verify fails, drop the trace and log the failure

The Qwen3 chat-template format produced is OpenAI-style:
    {
      "messages": [
        {"role": "system", "content": "..."},
        {"role": "user", "content": "..."},
        {"role": "assistant", "content": "...", "tool_calls": [...]},
        {"role": "tool", "tool_call_id": "...", "content": "..."},
        ...
      ],
      "tools": [...]
    }

Usage:
    python3 distill.py \
        --tasks bench/finetune/qwen3_coder_30b/tasks \
        --output bench/finetune/qwen3_coder_30b/data/validation_train.jsonl \
        --max-tasks 60

Cost: ~$0.20 per task on Sonnet 4.6.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
NEBULA_BIN = REPO_ROOT / "compiler" / "target" / "release" / "nebula"
NEBULA_MCP_CONFIG = REPO_ROOT / "bench" / "mcp_manual_test" / "nebula_mcp.json"


CLAUDE_P_DISALLOWED = " ".join([
    # Built-in file/edit tools — we want everything to go through MCP
    "Read", "Edit", "Write", "Grep", "MultiEdit", "NotebookEdit", "Glob",
    # Planning/agent tools — we don't want them in Qwen training data
    "TodoWrite", "Task", "ToolSearch", "WebFetch", "WebSearch",
    "ExitPlanMode", "EnterPlanMode", "EnterWorktree", "ExitWorktree",
    "AskUserQuestion", "Skill", "ScheduleWakeup",
    "CronCreate", "CronDelete", "CronList", "RemoteTrigger",
    "Monitor", "TaskOutput", "TaskStop", "LSP",
])


def run_claude_p(task_dir, prompt, output_jsonl, max_budget=2.0):
    """Run a one-shot claude -p session with Nebula MCP enabled.
    Tool surface is restricted to: Bash + the 5 mcp__nebula__* tools."""
    cmd = [
        "claude", "--print",
        "--mcp-config", str(NEBULA_MCP_CONFIG),
        "--strict-mcp-config",
        "--disallowed-tools", CLAUDE_P_DISALLOWED,
        "--permission-mode", "bypassPermissions",
        "--model", "sonnet",
        "--output-format", "stream-json",
        "--verbose",
        "--max-budget-usd", str(max_budget),
        "--no-session-persistence",
        "--setting-sources", "user",
        prompt,
    ]
    with open(output_jsonl, "w") as out:
        proc = subprocess.run(
            cmd, cwd=task_dir,
            stdout=out, stderr=subprocess.PIPE,
            timeout=600,
        )
    return proc.returncode == 0, proc.stderr.decode(errors="replace")[:1000]


def run_verify(verify_cmd, task_dir, timeout=120):
    p = subprocess.run(
        verify_cmd, shell=True, cwd=task_dir,
        capture_output=True, text=True, timeout=timeout,
    )
    return p.returncode == 0, (p.stdout + p.stderr)[-2000:]


# ── Stream-json → Qwen chat-template conversion ────────────────────────────

def parse_stream_json(stream_path):
    """Read a claude -p stream-json file and return (messages, tools, final_text, usage)."""
    messages = []
    tools = None
    final_text = ""
    final_usage = None

    for raw in open(stream_path):
        raw = raw.strip()
        if not raw:
            continue
        try:
            ev = json.loads(raw)
        except Exception:
            continue

        if ev.get("type") == "system" and ev.get("subtype") == "init":
            t = ev.get("tools") or []
            if t:
                tools = t
            continue

        if ev.get("type") == "result":
            final_usage = ev.get("usage")
            final_text = (ev.get("result") or "")[:4000]
            continue

        msg = ev.get("message")
        if not isinstance(msg, dict):
            continue

        role = msg.get("role")
        if role not in ("user", "assistant"):
            continue
        content = msg.get("content")
        if not isinstance(content, list):
            continue

        if role == "assistant":
            text_parts = []
            tool_calls = []
            for block in content:
                btype = block.get("type")
                if btype == "text":
                    text_parts.append(block.get("text", ""))
                elif btype == "tool_use":
                    tool_calls.append({
                        "id": block.get("id", ""),
                        "type": "function",
                        "function": {
                            "name": block.get("name", ""),
                            "arguments": json.dumps(block.get("input", {})),
                        },
                    })
            assistant_msg = {"role": "assistant", "content": "\n".join(text_parts).strip()}
            if tool_calls:
                assistant_msg["tool_calls"] = tool_calls
            messages.append(assistant_msg)

        elif role == "user":
            for block in content:
                btype = block.get("type")
                if btype == "tool_result":
                    tc_id = block.get("tool_use_id", "")
                    inner = block.get("content")
                    text = ""
                    if isinstance(inner, str):
                        text = inner
                    elif isinstance(inner, list):
                        for sub in inner:
                            if isinstance(sub, dict) and sub.get("type") == "text":
                                text += sub.get("text", "")
                    messages.append({
                        "role": "tool",
                        "tool_call_id": tc_id,
                        "content": text,
                    })
                elif btype == "text":
                    messages.append({"role": "user", "content": block.get("text", "")})

    return messages, tools, final_text, final_usage


QWEN_TRAINING_SYSTEM_PROMPT = """You are an autonomous coding agent. You have access to tools for reading, searching, writing, and patching code, plus a bash tool for running test/build commands.

Working principles:
- Call tools decisively. Don't narrate; just call the tool and look at the result.
- Prefer patch_code over write_code for any change smaller than a full file rewrite. Use replace_all=true for renames or repetitive substitutions.
- After making changes, run the verify command (go test, pytest, etc.) to confirm. If it fails, fix the bug and re-run.
- When the task is fully complete and the verification passes, respond with a brief plain-text summary and stop calling tools."""


def fetch_mcp_tools_as_openai_schema():
    """Spawn the Nebula MCP server, call tools/list, and convert each tool to
    OpenAI function-calling schema. Adds a synthetic `bash` tool to match what
    host_agent_runner.py exposes at inference time."""
    proc = subprocess.Popen(
        [str(NEBULA_BIN), "mcp"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        bufsize=0,
    )
    try:
        for req in [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
        ]:
            proc.stdin.write((json.dumps(req) + "\n").encode())
            proc.stdin.flush()
        # Read responses (skip the notification which has no reply)
        responses = []
        for _ in range(2):
            line = proc.stdout.readline()
            if not line:
                break
            responses.append(json.loads(line))
        tools_resp = next((r for r in responses if r.get("id") == 2), None)
        if not tools_resp:
            return []
        mcp_tools = tools_resp.get("result", {}).get("tools", [])
    finally:
        proc.stdin.close()
        proc.terminate()
        proc.wait(timeout=3)

    out = []
    for t in mcp_tools:
        # Tool names exposed via Claude Code's MCP layer get a `mcp__<server>__` prefix.
        out.append({
            "type": "function",
            "function": {
                "name": f"mcp__nebula__{t['name']}",
                "description": t.get("description", ""),
                "parameters": t.get("inputSchema", {"type": "object", "properties": {}}),
            },
        })

    # Synthetic bash tool — matches what host_agent_runner.py exposes
    out.append({
        "type": "function",
        "function": {
            "name": "Bash",
            "description": "Run a shell command in the working directory and return its output (stdout+stderr combined). Use this for go test, pytest, cargo check, ls, etc.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Shell command to execute"},
                    "description": {"type": "string", "description": "Short description of why you're running this"},
                },
                "required": ["command"],
            },
        },
    })
    return out


def to_qwen_chat_format(messages, canonical_tools, system_prompt):
    """Convert a parsed claude -p stream into Qwen3 fine-tuning format.
    `canonical_tools` is the inference-time tool list (from fetch_mcp_tools_as_openai_schema).

    Post-processes the message list:
    - Drops empty assistant messages (no content AND no tool_calls)
    - Merges consecutive assistant messages into a single message
      (content joined with "\n", tool_calls concatenated). Qwen3 / OpenAI
      chat templates expect alternating roles; consecutive assistant turns
      break tokenization.
    """
    cleaned = []
    for m in messages:
        role = m.get("role")
        if role == "assistant":
            if not m.get("content") and not m.get("tool_calls"):
                continue
        cleaned.append(m)

    merged = []
    for m in cleaned:
        if (merged
            and m.get("role") == "assistant"
            and merged[-1].get("role") == "assistant"):
            prev = merged[-1]
            # Concatenate text content
            prev_content = prev.get("content") or ""
            new_content = m.get("content") or ""
            if prev_content and new_content:
                prev["content"] = prev_content + "\n" + new_content
            elif new_content:
                prev["content"] = new_content
            # Union tool calls
            prev_tcs = prev.get("tool_calls") or []
            new_tcs = m.get("tool_calls") or []
            if prev_tcs or new_tcs:
                prev["tool_calls"] = prev_tcs + new_tcs
        else:
            merged.append(dict(m))  # copy so we don't mutate the input

    out_messages = [{"role": "system", "content": system_prompt}] + merged
    return {"messages": out_messages, "tools": canonical_tools}


def validate_trace(trace, task_meta):
    msgs = trace.get("messages", [])
    if len(msgs) < 3:
        return False, "fewer than 3 messages"

    has_assistant = False
    has_tool_call = False
    for m in msgs:
        if m.get("role") == "assistant":
            has_assistant = True
            for tc in m.get("tool_calls") or []:
                has_tool_call = True
                args = tc.get("function", {}).get("arguments", "")
                if isinstance(args, str):
                    try:
                        json.loads(args)
                    except Exception:
                        return False, f"unparseable tool call arguments: {args[:100]}"

    if not has_assistant:
        return False, "no assistant messages"
    if not has_tool_call:
        return False, "no tool calls in trace"
    return True, "ok"


def check_explain_keywords(messages, expected_keywords):
    final = ""
    for m in messages:
        if m.get("role") == "assistant" and m.get("content"):
            final = m["content"]
    if not expected_keywords:
        return True
    hits = sum(1 for k in expected_keywords if k.lower() in final.lower())
    return hits >= max(1, len(expected_keywords) // 2)


def _log_failure(fp, task_id, reason, detail):
    print(f"    FAIL ({reason}): {str(detail)[:200]}", flush=True)
    if fp:
        fp.write(json.dumps({"task": task_id, "reason": reason, "detail": str(detail)[:1000]}) + "\n")


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--tasks", required=True)
    p.add_argument("--output", required=True)
    p.add_argument("--failures-log", default=None)
    p.add_argument("--max-tasks", type=int, default=200)
    p.add_argument("--skip-existing", action="store_true")
    p.add_argument("--dry-run", action="store_true")
    args = p.parse_args()

    tasks_root = Path(args.tasks)
    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fail_log = open(args.failures_log, "a") if args.failures_log else None

    existing = set()
    if args.skip_existing and out_path.exists():
        for line in open(out_path):
            try:
                rec = json.loads(line)
                if rec.get("_task_id"):
                    existing.add(rec["_task_id"])
            except Exception:
                pass

    index_path = tasks_root / "task_index.json"
    if index_path.exists():
        index = json.loads(index_path.read_text())
        all_ids = index.get("tasks", [])
    else:
        all_ids = sorted(d.name for d in tasks_root.iterdir() if d.is_dir())

    pending = [t for t in all_ids if t not in existing][:args.max_tasks]
    print(f"Distillation: {len(pending)} tasks pending (output: {out_path})", flush=True)
    if not pending:
        return

    if args.dry_run:
        for t in pending[:5]:
            print(f"  would run: {t}")
        return

    # Pre-compute the canonical tool schema from a one-shot MCP server probe.
    # This is the EXACT tool list the inference-time runner will expose, so
    # the training data won't drift from inference reality.
    print("Probing MCP server for tool schema...", flush=True)
    canonical_tools = fetch_mcp_tools_as_openai_schema()
    print(f"  got {len(canonical_tools)} canonical tools: {[t['function']['name'] for t in canonical_tools]}", flush=True)

    out_fp = open(out_path, "a")
    stats = {"pass": 0, "verify_fail": 0, "explain_fail": 0, "validate_fail": 0, "tool_drift": 0, "claude_fail": 0, "exception": 0}
    t_start = time.time()
    canonical_tool_names = {t["function"]["name"] for t in canonical_tools}

    for i, task_id in enumerate(pending, 1):
        task_dir = tasks_root / task_id
        meta = json.loads((task_dir / "meta.json").read_text())
        seed = task_dir / "seed"
        prompt = meta["prompt"]
        verify_cmd = meta["verify_cmd"]

        trial = Path(tempfile.mkdtemp(prefix=f"distill_{task_id}_"))
        try:
            shutil.copytree(seed, trial / "work", dirs_exist_ok=False)
            work_dir = trial / "work"

            stream_path = trial / "stream.jsonl"
            print(f"[{i}/{len(pending)}] {task_id} ({meta['type']})", flush=True)

            t0 = time.time()
            ok, stderr = run_claude_p(work_dir, prompt, stream_path)
            wall = time.time() - t0

            if not ok and not stream_path.exists():
                stats["claude_fail"] += 1
                _log_failure(fail_log, task_id, "claude_p_failed", stderr)
                continue

            v_ok, v_out = run_verify(verify_cmd, work_dir)

            try:
                messages, tools, final_text, usage = parse_stream_json(stream_path)
            except Exception as e:
                stats["exception"] += 1
                _log_failure(fail_log, task_id, "parse_exception", str(e))
                continue

            if meta["type"] == "explain":
                if not check_explain_keywords(messages, meta.get("expected_keywords_in_final", [])):
                    stats["explain_fail"] += 1
                    _log_failure(fail_log, task_id, "explain_keywords_missing", final_text[:300])
                    continue
            else:
                if not v_ok:
                    stats["verify_fail"] += 1
                    _log_failure(fail_log, task_id, "verify_failed", v_out[-500:])
                    continue

            trace = to_qwen_chat_format(messages, canonical_tools, QWEN_TRAINING_SYSTEM_PROMPT)
            valid, reason = validate_trace(trace, meta)
            if not valid:
                stats["validate_fail"] += 1
                _log_failure(fail_log, task_id, f"validate: {reason}", "")
                continue

            # Tool-drift check: every tool call in the trace must use a tool we
            # actually expose at inference time. If Sonnet snuck in a Glob / Task
            # / etc. (via the disallowed-tools fence somehow being incomplete),
            # the trace teaches the wrong thing — drop it.
            drift = []
            for m in trace["messages"]:
                for tc in m.get("tool_calls") or []:
                    name = tc.get("function", {}).get("name", "")
                    if name not in canonical_tool_names:
                        drift.append(name)
            if drift:
                stats["tool_drift"] += 1
                _log_failure(fail_log, task_id, "tool_drift", f"unexpected tools: {set(drift)}")
                continue

            trace["_task_id"] = task_id
            trace["_task_type"] = meta["type"]
            trace["_task_language"] = meta.get("language", "")
            trace["_wall_seconds"] = round(wall, 1)
            out_fp.write(json.dumps(trace) + "\n")
            out_fp.flush()
            stats["pass"] += 1
            print(f"    OK in {wall:.1f}s ({len(trace['messages'])} msgs)", flush=True)

        except subprocess.TimeoutExpired:
            stats["exception"] += 1
            _log_failure(fail_log, task_id, "timeout", "")
        except Exception as e:
            stats["exception"] += 1
            _log_failure(fail_log, task_id, "exception", str(e))
        finally:
            try:
                shutil.rmtree(trial)
            except Exception:
                pass

    out_fp.close()
    if fail_log:
        fail_log.close()

    elapsed = time.time() - t_start
    print(flush=True)
    print(f"=== Distillation summary ({elapsed/60:.1f} min) ===", flush=True)
    for k, v in stats.items():
        print(f"  {k:14}: {v}", flush=True)
    print(f"  output:        {out_path}", flush=True)


if __name__ == "__main__":
    main()
