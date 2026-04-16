#!/usr/bin/env python3
"""Distillation runner — drives Qwen3-Coder-480B (Apache 2.0, via Together AI)
through Nebula MCP on each task and captures the agent traces as fine-tuning
data.

For every task in tasks/:
    1. Copy seed/ to a fresh trial directory
    2. Run the agent loop against Qwen3-Coder-480B-A35B-Instruct on Together
       with the task's prompt against that trial dir
    3. Run the verify command in the trial dir
    4. If verify passes (or for explain tasks, the final message contains the
       expected keywords), convert the messages into Qwen3 chat-template
       format and append to the output JSONL
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

Teacher choice: Qwen3-Coder-480B-A35B-Instruct is Apache 2.0 and its license
permits distillation into derivative models. Anthropic's Claude models are
NOT usable as teachers here — their consumer/commercial terms prohibit using
model outputs to train any other ML model (see
https://www.anthropic.com/legal/consumer-terms §3). Do not add a Claude
teacher path; do not feed Claude-derived traces into this pipeline.

Cost: ~$0.10-0.30 per task on Qwen3-Coder-480B via Together AI.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
NEBULA_BIN = REPO_ROOT / "compiler" / "target" / "release" / "nebula"


# ── Together AI / Qwen3-Coder-480B teacher driver ─────────────────────────

TOGETHER_URL = "https://api.together.xyz/v1/chat/completions"
TOGETHER_MODEL = "Qwen/Qwen3-Coder-480B-A35B-Instruct-FP8"


def _load_together_key():
    env = REPO_ROOT / ".env"
    if env.exists():
        for line in env.read_text().splitlines():
            if line.startswith("TOGETHER_API_KEY="):
                return line.split("=", 1)[1].strip().strip('"').strip("'")
    return os.environ.get("TOGETHER_API_KEY", "")


class MCPSession:
    """Persistent Nebula MCP subprocess for one task. JSON-RPC over stdio."""

    def __init__(self, cwd):
        self.proc = subprocess.Popen(
            [str(NEBULA_BIN), "mcp"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            cwd=cwd, bufsize=0,
        )
        self._next_id = 1
        self._send({"jsonrpc": "2.0", "id": self._next_id, "method": "initialize", "params": {}})
        self._next_id += 1
        self._recv_until(1)
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def _send(self, obj):
        self.proc.stdin.write((json.dumps(obj) + "\n").encode())
        self.proc.stdin.flush()

    def _recv_until(self, want_id):
        while True:
            line = self.proc.stdout.readline()
            if not line:
                return None
            try:
                msg = json.loads(line)
            except Exception:
                continue
            if msg.get("id") == want_id:
                return msg

    def call_tool(self, name, arguments):
        rid = self._next_id
        self._next_id += 1
        self._send({
            "jsonrpc": "2.0",
            "id": rid,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments},
        })
        resp = self._recv_until(rid)
        if resp is None:
            return "ERROR: MCP process died"
        if "error" in resp:
            err = resp["error"]
            return f"ERROR: {err.get('message', str(err))}"
        result = resp.get("result", {})
        content = result.get("content") or []
        parts = []
        for c in content:
            if isinstance(c, dict) and c.get("type") == "text":
                parts.append(c.get("text", ""))
        if result.get("isError"):
            return "ERROR: " + "\n".join(parts)
        return "\n".join(parts) if parts else json.dumps(result)

    def close(self):
        try:
            self.proc.stdin.close()
        except Exception:
            pass
        try:
            self.proc.terminate()
            self.proc.wait(timeout=3)
        except Exception:
            try:
                self.proc.kill()
            except Exception:
                pass


def _run_bash(command, cwd, timeout=120):
    try:
        p = subprocess.run(
            command, shell=True, cwd=cwd,
            capture_output=True, text=True, timeout=timeout,
        )
        out = (p.stdout + p.stderr)[-4000:]
        if not out:
            out = f"(exit {p.returncode}, no output)"
        return out
    except subprocess.TimeoutExpired:
        return f"ERROR: command timed out after {timeout}s"
    except Exception as e:
        return f"ERROR: {type(e).__name__}: {e}"


def _together_chat(api_key, messages, tools, max_retries=3):
    body = {
        "model": TOGETHER_MODEL,
        "messages": messages,
        "tools": tools,
        "tool_choice": "auto",
        "temperature": 0.1,
        "max_tokens": 2048,
    }
    data = json.dumps(body).encode()
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
        "User-Agent": "nebula-distill/1.0 (phase-11-teacher-swap)",
        "Accept": "application/json",
    }
    for attempt in range(max_retries):
        try:
            req = urllib.request.Request(TOGETHER_URL, data=data, headers=headers, method="POST")
            with urllib.request.urlopen(req, timeout=180) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as e:
            body_text = ""
            try:
                body_text = e.read().decode(errors="replace")[:500]
            except Exception:
                pass
            if e.code in (429, 500, 502, 503, 504) and attempt < max_retries - 1:
                time.sleep(2 ** attempt)
                continue
            raise RuntimeError(f"Together HTTP {e.code}: {body_text}")
        except Exception as e:
            if attempt < max_retries - 1:
                time.sleep(2 ** attempt)
                continue
            raise
    raise RuntimeError("Together retries exhausted")


def run_teacher_together(work_dir, prompt, canonical_tools, max_turns=30, system_prompt_variant="default"):
    """Drive Qwen3-Coder-480B through the Nebula MCP agent loop.

    Returns (messages, final_text, stats). `messages` is the full list in
    OpenAI-chat format (system, user, assistant-with-tool_calls, tool, ...),
    which feeds directly into to_qwen_chat_format.
    """
    api_key = _load_together_key()
    if not api_key:
        raise RuntimeError("TOGETHER_API_KEY not set")

    mcp = MCPSession(work_dir)
    try:
        # The system prompt is added to the teacher request but NOT returned in
        # the messages list — to_qwen_chat_format adds the training-time system
        # prompt itself, so we'd end up with two copies otherwise.
        system_msg = {"role": "system", "content": SYSTEM_PROMPT_VARIANTS[system_prompt_variant]}
        messages = [{"role": "user", "content": prompt}]
        stats = {"turns": 0, "tool_calls": 0, "input_tokens": 0, "output_tokens": 0}
        final_text = ""

        for turn in range(max_turns):
            stats["turns"] = turn + 1
            resp = _together_chat(api_key, [system_msg] + messages, canonical_tools)
            usage = resp.get("usage") or {}
            stats["input_tokens"] += usage.get("prompt_tokens", 0)
            stats["output_tokens"] += usage.get("completion_tokens", 0)

            choice = (resp.get("choices") or [{}])[0]
            msg = choice.get("message") or {}
            content = msg.get("content") or ""
            tool_calls = msg.get("tool_calls") or []

            assistant_msg = {"role": "assistant", "content": content}
            if tool_calls:
                # Normalize tool_calls to the OpenAI-style shape that
                # to_qwen_chat_format expects downstream.
                norm_tcs = []
                for tc in tool_calls:
                    fn = tc.get("function") or {}
                    args_raw = fn.get("arguments", "")
                    if isinstance(args_raw, dict):
                        args_raw = json.dumps(args_raw)
                    norm_tcs.append({
                        "id": tc.get("id") or f"call_{turn}_{len(norm_tcs)}",
                        "type": "function",
                        "function": {
                            "name": fn.get("name", ""),
                            "arguments": args_raw,
                        },
                    })
                assistant_msg["tool_calls"] = norm_tcs
            messages.append(assistant_msg)

            if not tool_calls:
                final_text = content
                break

            # Dispatch each tool call and append a tool message
            for tc in assistant_msg["tool_calls"]:
                stats["tool_calls"] += 1
                fn = tc["function"]
                name = fn["name"]
                try:
                    args = json.loads(fn["arguments"]) if fn["arguments"] else {}
                    if not isinstance(args, dict):
                        args = {}
                except Exception:
                    args = {}

                if name == "Bash":
                    result = _run_bash(args.get("command", ""), cwd=work_dir)
                elif name.startswith("mcp__nebula__"):
                    mcp_name = name[len("mcp__nebula__"):]
                    result = mcp.call_tool(mcp_name, args)
                else:
                    result = f"ERROR: unknown tool {name}"

                messages.append({
                    "role": "tool",
                    "tool_call_id": tc["id"],
                    "content": result[:8000],
                })

        return messages, final_text, stats
    finally:
        mcp.close()


def run_verify(verify_cmd, task_dir, timeout=120):
    p = subprocess.run(
        verify_cmd, shell=True, cwd=task_dir,
        capture_output=True, text=True, timeout=timeout,
    )
    return p.returncode == 0, (p.stdout + p.stderr)[-2000:]


RENAME_DISCIPLINE_BLOCK = """

## Rename-task discipline

If the user's task is to rename a symbol (type, function, method, variable, constant) — whether the symbol appears in a single file or across many — you MUST:

1. Use `search_code` with the old name to discover every file that references it.
2. For each file in the results, call `patch_code` exactly ONCE with `replace_all=true`. The `old_str` should be the bare symbol name (or a minimally distinguishing snippet) and `new_str` the replacement name.
3. Do NOT emit individual `patch_code` calls against the same file. Even if a file has only one reference, prefer one `replace_all=true` call over a hand-crafted single-occurrence patch.
4. After all files are patched, run the verification command (go build / go test / etc.) ONCE and respond with a brief final summary.

A rename of N references across M files is exactly M `patch_code(replace_all=true)` calls, not N individual patches. For a single-file rename with 4 references, that is 1 `patch_code(replace_all=true)` call, not 4."""


QWEN_TRAINING_SYSTEM_PROMPT = """You are an autonomous coding agent. You have access to tools for reading, searching, writing, and patching code, plus a bash tool for running test/build commands.

IMPORTANT — about read_code and patch_code:
- For large files (>100 lines), read_code returns source in Nebula form — a compact canonical IL you can recognize by `fn`, `rt`, `fo`, `def`, `:=`, indent blocks instead of braces, and `Str`/`Int`/`Bool` type names. Small files come back as raw native source.
- patch_code ALWAYS matches against the native source bytes (Go, Python, etc.), NEVER Nebula. If read_code showed you Nebula for a file, you must mentally translate each `old_str` back to the native syntax before calling patch_code. If you see `fn Foo(s: Str) -> Str:` in the Nebula read, your `old_str` must be the native Go form `func Foo(s string) string {` — with real braces, real types, and real indentation.
- If patch_code returns `old_str not found`, do NOT retry the same old_str. Re-think: you probably copied from a Nebula read. Use bash `cat` (or run the test to see the raw error) to confirm the native syntax, then try again.

Working principles:
- Call tools decisively. Don't narrate; just call the tool and look at the result.
- Prefer patch_code over write_code for any change smaller than a full file rewrite.
- For ANY rename across multiple files, use exactly one `patch_code` call per affected file with `replace_all=true`. That's almost always 5-15 tool calls total, not 30+. Individual patches without replace_all are only correct when the symbol appears in just one or two places total OR when you need surgical context-specific control.
- After making changes, run the verify command (go test, pytest, etc.) to confirm. If it fails, fix the bug and re-run.
- BEFORE responding with your final summary, run the verification command one last time. A green build is the ONLY signal that you're done — if `go build` or the tests still fail, you have more work to do, not a summary to write.
- When the task is fully complete and the verification passes, respond with a brief plain-text summary and stop calling tools."""


QWEN_REPLACE_ALL_SYSTEM_PROMPT = QWEN_TRAINING_SYSTEM_PROMPT + RENAME_DISCIPLINE_BLOCK


SYSTEM_PROMPT_VARIANTS = {
    "default": QWEN_TRAINING_SYSTEM_PROMPT,
    "replace_all_mandatory": QWEN_REPLACE_ALL_SYSTEM_PROMPT,
}


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
    """Convert a normalized OpenAI-style message list into Qwen3 fine-tuning format.
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
    # Teacher is fixed to qwen3-coder-480b (Apache 2.0, distillation permitted).
    # DO NOT re-add a Claude teacher path: Anthropic's consumer/commercial terms
    # §3 prohibit using Claude outputs to train any other ML model.
    p.add_argument("--only-task", default=None,
                   help="Run just one task (by task_id) — for smoke testing")
    p.add_argument("--only-tasks", default=None,
                   help="Comma-separated list of task_ids to run (for phase 12 refactor-only rerun)")
    p.add_argument("--only-type", default=None,
                   choices=["implement", "refactor", "bugfix", "explain", "cross_lang"],
                   help="Only run tasks of this type")
    p.add_argument("--system-prompt-variant", default="default",
                   choices=list(SYSTEM_PROMPT_VARIANTS.keys()),
                   help="Which teacher system prompt to use")
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

    if args.only_task:
        pending = [args.only_task] if args.only_task in all_ids else []
    elif args.only_tasks:
        wanted = set(t.strip() for t in args.only_tasks.split(","))
        pending = [t for t in all_ids if t in wanted]
    else:
        pending = [t for t in all_ids if t not in existing]
        if args.only_type:
            filtered = []
            for tid in pending:
                meta_path = tasks_root / tid / "meta.json"
                if meta_path.exists():
                    try:
                        if json.loads(meta_path.read_text()).get("type") == args.only_type:
                            filtered.append(tid)
                    except Exception:
                        pass
            pending = filtered
        pending = pending[:args.max_tasks]
    print(f"Distillation: teacher=qwen3-coder-480b variant={args.system_prompt_variant}, {len(pending)} tasks pending (output: {out_path})", flush=True)
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
    stats = {"pass": 0, "verify_fail": 0, "explain_fail": 0, "validate_fail": 0, "tool_drift": 0, "teacher_fail": 0, "exception": 0}
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

            print(f"[{i}/{len(pending)}] {task_id} ({meta['type']})", flush=True)

            t0 = time.time()
            try:
                messages, final_text, teacher_stats = run_teacher_together(
                    work_dir, prompt, canonical_tools, max_turns=30,
                    system_prompt_variant=args.system_prompt_variant,
                )
                wall = time.time() - t0
                print(f"    teacher: turns={teacher_stats['turns']} "
                      f"tools={teacher_stats['tool_calls']} "
                      f"in={teacher_stats['input_tokens']} "
                      f"out={teacher_stats['output_tokens']}", flush=True)
            except Exception as e:
                stats["teacher_fail"] += 1
                _log_failure(fail_log, task_id, "teacher_failed", str(e))
                continue

            v_ok, v_out = run_verify(verify_cmd, work_dir)

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
            # actually expose at inference time. If the teacher called a tool
            # outside the canonical set, the trace would teach the student to
            # reach for tools that don't exist at inference time — drop it.
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
