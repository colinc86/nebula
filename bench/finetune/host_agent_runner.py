#!/usr/bin/env python3
"""Host coding agent runner — drives an Ollama-served model through the Nebula MCP.

Replaces `claude -p` for benchmarking purposes. Speaks ollama's /api/chat with
tools, dispatches the model's tool calls to the Nebula MCP server over stdio,
feeds results back, and loops until the model produces a non-tool response or
hits the turn cap.

Usage:
    python3 host_agent_runner.py \
        --model qwen3-coder:30b \
        --ollama-url http://192.168.68.94:11434 \
        --mcp-command /path/to/nebula mcp \
        --task-dir /tmp/mcp_test/baseline_v8 \
        --prompt-file ../mcp_manual_test/prompt.txt \
        --verify "go test ./..." \
        --output /tmp/qwen_implement.jsonl

Outputs a JSONL log of every turn (model message + tool calls + tool results),
plus a final summary line with token totals, turn count, wall clock, and verify
exit code. Designed to be parseable by the same harness that reads `claude -p`
stream-json output, so we can compare apples-to-apples.

No external dependencies — stdlib only (urllib, subprocess, json).
"""

import argparse
import json
import os
import re
import shlex
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

# ── MCP client (JSON-RPC over stdio) ────────────────────────────────────────

class MCPClient:
    """Minimal JSON-RPC 2.0 client for an MCP server speaking over stdio."""

    def __init__(self, command, env=None):
        self.command = command
        self.env = {**os.environ, **(env or {})}
        self.proc = None
        self.req_id = 0
        self.instructions = ""
        self.tools = []

    def start(self, cwd=None):
        self.proc = subprocess.Popen(
            self.command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=self.env,
            cwd=cwd,
            bufsize=0,
        )

    def stop(self):
        if self.proc:
            try:
                self.proc.stdin.close()
                self.proc.terminate()
                self.proc.wait(timeout=5)
            except Exception:
                self.proc.kill()
            self.proc = None

    def _call(self, method, params=None):
        self.req_id += 1
        msg = {"jsonrpc": "2.0", "id": self.req_id, "method": method}
        if params is not None:
            msg["params"] = params
        line = (json.dumps(msg) + "\n").encode()
        self.proc.stdin.write(line)
        self.proc.stdin.flush()
        # Read one line of response (MCP server emits one JSON object per request)
        raw = self.proc.stdout.readline()
        if not raw:
            stderr = self.proc.stderr.read().decode(errors="replace") if self.proc.stderr else ""
            raise RuntimeError(f"MCP server closed stdout. stderr: {stderr[:500]}")
        return json.loads(raw)

    def _notify(self, method, params=None):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        line = (json.dumps(msg) + "\n").encode()
        self.proc.stdin.write(line)
        self.proc.stdin.flush()

    def initialize(self):
        resp = self._call("initialize", {})
        result = resp.get("result", {})
        self.instructions = result.get("instructions", "") or ""
        # Per MCP spec, send notifications/initialized (no response expected)
        self._notify("notifications/initialized")
        return result

    def list_tools(self):
        resp = self._call("tools/list", {})
        self.tools = resp.get("result", {}).get("tools", [])
        return self.tools

    def call_tool(self, name, arguments):
        resp = self._call("tools/call", {"name": name, "arguments": arguments})
        if "error" in resp:
            err = resp["error"]
            return False, f"MCP error: {err.get('message', err)}"
        result = resp.get("result", {})
        if result.get("isError"):
            text = ""
            for block in result.get("content", []):
                if block.get("type") == "text":
                    text += block.get("text", "")
            return False, text
        text = ""
        for block in result.get("content", []):
            if block.get("type") == "text":
                text += block.get("text", "")
        return True, text


# ── Ollama client (HTTP /api/chat) ──────────────────────────────────────────

class OllamaClient:
    def __init__(self, base_url, model, num_ctx=32768, temperature=0.1):
        self.base_url = base_url.rstrip("/")
        self.model = model
        self.num_ctx = num_ctx
        self.temperature = temperature

    def chat(self, messages, tools, timeout=300):
        payload = {
            "model": self.model,
            "messages": messages,
            "tools": tools,
            "stream": False,
            "options": {
                "num_ctx": self.num_ctx,
                "temperature": self.temperature,
                "num_predict": 2048,
            },
        }
        req = urllib.request.Request(
            f"{self.base_url}/api/chat",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read())


# ── OpenAI-compatible client (Together AI, vLLM, anything that speaks /v1/chat/completions) ──

class OpenAIClient:
    """Minimal OpenAI-compatible chat completion client. Used for Together AI
    dedicated endpoints, vLLM, or any OpenAI-shaped API. The response is
    normalized back to ollama's shape so the agent loop doesn't have to care."""

    def __init__(self, base_url, model, api_key=None, temperature=0.1, max_tokens=2048):
        self.base_url = base_url.rstrip("/")
        self.model = model
        self.api_key = api_key
        self.temperature = temperature
        self.max_tokens = max_tokens

    def chat(self, messages, tools, timeout=300):
        payload = {
            "model": self.model,
            "messages": messages,
            "tools": tools,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "stream": False,
        }
        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        req = urllib.request.Request(
            f"{self.base_url}/v1/chat/completions",
            data=json.dumps(payload).encode(),
            headers=headers,
        )
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = json.loads(resp.read())

        # Normalize OpenAI shape → ollama shape the loop expects:
        #   ollama:  {"message": {"role":.., "content":.., "tool_calls":[...]}, "prompt_eval_count":.., "eval_count":..}
        #   openai:  {"choices":[{"message":{...}}], "usage":{"prompt_tokens":.., "completion_tokens":..}}
        if "choices" in data and data["choices"]:
            msg = data["choices"][0].get("message", {})
        else:
            msg = {}
        usage = data.get("usage") or {}
        return {
            "message": msg,
            "prompt_eval_count": usage.get("prompt_tokens", 0),
            "eval_count": usage.get("completion_tokens", 0),
            "total_duration": 0,
        }


# ── Tool format conversion ──────────────────────────────────────────────────

def mcp_tool_to_ollama(mcp_tool):
    """Convert an MCP tool descriptor into ollama's OpenAI-style tool schema."""
    return {
        "type": "function",
        "function": {
            "name": mcp_tool["name"],
            "description": mcp_tool.get("description", ""),
            "parameters": mcp_tool.get("inputSchema", {"type": "object", "properties": {}}),
        },
    }


# ── Qwen XML tool call fallback parser ──────────────────────────────────────

QWEN_FUNCTION_RE = re.compile(
    r"<function=(?P<name>[A-Za-z0-9_]+)>(?P<body>.*?)</function>",
    re.DOTALL,
)
QWEN_PARAM_RE = re.compile(
    r"<parameter=(?P<key>[A-Za-z0-9_]+)>(?P<value>.*?)</parameter>",
    re.DOTALL,
)


def parse_qwen_xml_tool_calls(content):
    """Pull Qwen-style tool calls out of the assistant `content` when ollama
    didn't structure them. Returns a list shaped like ollama's tool_calls."""
    if not content:
        return []
    out = []
    for i, m in enumerate(QWEN_FUNCTION_RE.finditer(content)):
        name = m.group("name")
        body = m.group("body")
        args = {}
        for pm in QWEN_PARAM_RE.finditer(body):
            args[pm.group("key")] = pm.group("value").strip()
        out.append({
            "id": f"qxml_{i}",
            "function": {"name": name, "arguments": args},
        })
    return out


# ── Agent loop ──────────────────────────────────────────────────────────────

# Matches distill.py's QWEN_TRAINING_SYSTEM_PROMPT exactly. The fine-tuned
# model saw this prompt in every training example, so we use the same one at
# inference to avoid prompt drift. For the stock-model baseline comparison,
# using this prompt is also fine — it's a valid agentic system prompt that
# doesn't favor either model.
SYSTEM_PROMPT_PREFIX = """You are an autonomous coding agent. You have access to tools for reading, searching, writing, and patching code, plus a bash tool for running test/build commands.

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


def build_system_prompt(mcp_instructions, extra_tool_note):
    parts = [SYSTEM_PROMPT_PREFIX.strip()]
    if mcp_instructions:
        parts.append("== Tool server primer ==\n" + mcp_instructions.strip())
    if extra_tool_note:
        parts.append(extra_tool_note.strip())
    return "\n\n".join(parts)


def run_agent_loop(ollama, mcp, user_prompt, bash_cwd, max_turns=30, log_fp=None):
    """Drive the agent loop. Returns metrics dict + final assistant text."""

    init = mcp.initialize()
    mcp_tools = mcp.list_tools()
    ollama_tools = [mcp_tool_to_ollama(t) for t in mcp_tools]

    # Add a synthetic "bash" tool for running the verification command. The MCP
    # server doesn't expose one, so we surface it ourselves.
    ollama_tools.append({
        "type": "function",
        "function": {
            "name": "bash",
            "description": "Run a shell command in the working directory and return its output (stdout+stderr combined). Use this to run go test, pytest, cargo check, etc.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Shell command to execute"}
                },
                "required": ["command"]
            }
        }
    })

    extra_note = (
        f"== Working directory ==\n{bash_cwd}\n\n"
        "When you call `bash`, the command runs from this directory. "
        "Use it for `go test`, `go build`, `pytest`, etc."
    )
    system_prompt = build_system_prompt(mcp.instructions, extra_note)

    messages = [
        {"role": "system", "content": system_prompt},
        {"role": "user", "content": user_prompt},
    ]

    metrics = {
        "turns": 0,
        "tool_calls": 0,
        "tool_calls_by_name": {},
        "input_tokens": 0,
        "output_tokens": 0,
        "duration_ms": 0,
        "stopped_reason": "incomplete",
        "nudges_replace_all": 0,
        "nudges_stuck": 0,
    }
    final_text = ""

    # ── Path-B runtime nudge tracking ──────────────────────────────────────
    # Per-file counters for individual (non-replace_all) patch_code calls.
    # When we see ≥3 individual patches to the same file, we append a one-line
    # hint to the tool result pointing the model at replace_all=true. This is
    # a pure inference-time fix that doesn't require retraining.
    individual_patch_counts = {}   # path -> count of successful non-replace_all patch_code
    stuck_patch_fingerprints = {}  # path -> list of failed old_str prefixes (first 64 chars)
    NUDGE_INDIVIDUAL_THRESHOLD = 3
    NUDGE_STUCK_THRESHOLD = 2
    nudged_files = set()           # don't re-nudge the same file more than once per trigger type

    t0 = time.time()
    for turn in range(max_turns):
        metrics["turns"] += 1
        try:
            resp = ollama.chat(messages, ollama_tools)
        except (urllib.error.URLError, TimeoutError) as e:
            metrics["stopped_reason"] = f"ollama_error: {e}"
            break

        msg = resp.get("message", {})
        metrics["input_tokens"] += resp.get("prompt_eval_count", 0)
        metrics["output_tokens"] += resp.get("eval_count", 0)

        # Tool call extraction: prefer structured field, fall back to Qwen XML.
        tcs = msg.get("tool_calls") or []
        used_xml_fallback = False
        if not tcs:
            content = msg.get("content") or ""
            xml_tcs = parse_qwen_xml_tool_calls(content)
            if xml_tcs:
                tcs = xml_tcs
                used_xml_fallback = True
                # Strip the XML out of content so we don't double-feed it.
                msg["content"] = QWEN_FUNCTION_RE.sub("", content).strip()
                msg["tool_calls"] = tcs

        log_event(log_fp, {
            "type": "assistant",
            "turn": turn + 1,
            "content": msg.get("content", ""),
            "tool_calls": tcs,
            "used_xml_fallback": used_xml_fallback,
            "tokens": {
                "prompt_eval_count": resp.get("prompt_eval_count", 0),
                "eval_count": resp.get("eval_count", 0),
            },
        })

        if not tcs:
            # No tool call → model is done
            final_text = msg.get("content", "") or ""
            metrics["stopped_reason"] = "final_message"
            messages.append(msg)
            break

        # Append assistant turn (with tool_calls) and dispatch each call
        messages.append(msg)
        for tc in tcs:
            metrics["tool_calls"] += 1
            fn = tc.get("function", {})
            name = fn.get("name", "unknown")
            metrics["tool_calls_by_name"][name] = metrics["tool_calls_by_name"].get(name, 0) + 1

            args = fn.get("arguments", {})
            if isinstance(args, str):
                try:
                    args = json.loads(args)
                except Exception:
                    args = {"_raw": args}

            if name.lower() == "bash":
                ok, result_text = run_bash(args.get("command", ""), bash_cwd)
            else:
                # Strip the `mcp__nebula__` prefix (Claude Code's MCP convention)
                # so we pass the raw tool name to the MCP server.
                raw_name = name
                if raw_name.startswith("mcp__nebula__"):
                    raw_name = raw_name[len("mcp__nebula__"):]
                ok, result_text = mcp.call_tool(raw_name, args)

            # ── Path-B nudge injection ────────────────────────────────────
            # Look at the result AFTER dispatch. If the model is making many
            # individual patch_code calls to the same file, or retrying the
            # same failed old_str on one file, append a one-line hint.
            nudge_text = None
            if "patch_code" in name.lower():
                path = args.get("path", "")
                fname = path.split("/")[-1] if path else "?"
                replace_all_flag = bool(args.get("replace_all"))
                if ok and not replace_all_flag and path:
                    individual_patch_counts[path] = individual_patch_counts.get(path, 0) + 1
                    if (individual_patch_counts[path] >= NUDGE_INDIVIDUAL_THRESHOLD
                            and (path, "individual") not in nudged_files):
                        nudge_text = (
                            f"\n\n[runner hint] You've now made "
                            f"{individual_patch_counts[path]} individual patch_code calls "
                            f"to {fname}. If this is a rename or repetitive substitution, "
                            f"a single patch_code with replace_all=true (one call per file) "
                            f"is almost always the right strategy. Consider switching."
                        )
                        nudged_files.add((path, "individual"))
                        metrics["nudges_replace_all"] += 1
                if (not ok) and path and "not found" in result_text.lower():
                    # Stuck-loop detection: same old_str prefix seen twice failing.
                    old_prefix = (args.get("old_str", "") or "")[:64]
                    prior = stuck_patch_fingerprints.setdefault(path, [])
                    prior.append(old_prefix)
                    if (prior.count(old_prefix) >= NUDGE_STUCK_THRESHOLD
                            and (path, "stuck") not in nudged_files):
                        nudge_text = (
                            f"\n\n[runner hint] You've retried the same patch_code on "
                            f"{fname} and keep getting old_str not found. The file on disk "
                            f"is in NATIVE language (Go/Python). If read_code showed Nebula, "
                            f"you must translate `fn name(x: Str) -> Str:` to the native form "
                            f"like `func name(x string) string {{` before using it as old_str. "
                            f"Alternatively, use bash cat or re-read the file to see the exact "
                            f"bytes, then retry with the correct native syntax."
                        )
                        nudged_files.add((path, "stuck"))
                        metrics["nudges_stuck"] += 1

            log_event(log_fp, {
                "type": "tool_result",
                "turn": turn + 1,
                "tool": name,
                "args": args,
                "ok": ok,
                "result": result_text[:8000],
                "nudge": nudge_text,
            })

            messages.append({
                "role": "tool",
                "tool_call_id": tc.get("id", f"call_{turn}_{name}"),
                "name": name,
                "content": result_text + (nudge_text or ""),
            })
    else:
        metrics["stopped_reason"] = "max_turns"

    metrics["duration_ms"] = int((time.time() - t0) * 1000)
    return final_text, metrics


def run_bash(cmd, cwd, timeout=120):
    """Run a shell command and return (ok, combined_output)."""
    if not cmd.strip():
        return False, "empty command"
    try:
        proc = subprocess.run(
            cmd, shell=True, cwd=cwd,
            capture_output=True, text=True, timeout=timeout,
        )
        out = proc.stdout + proc.stderr
        return proc.returncode == 0, f"exit {proc.returncode}\n{out[:8000]}"
    except subprocess.TimeoutExpired:
        return False, f"TIMEOUT after {timeout}s"
    except Exception as e:
        return False, f"ERROR: {e}"


def log_event(fp, obj):
    if fp is None:
        return
    fp.write(json.dumps(obj, default=str) + "\n")
    fp.flush()


# ── Entry point ─────────────────────────────────────────────────────────────

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--model", required=True, help="Model tag. For ollama backend: 'qwen3-coder:30b' etc. For openai backend: the Together model_output_name or any model served at --openai-base-url")
    p.add_argument("--backend", choices=["ollama", "openai"], default="ollama", help="Which API shape to talk: ollama /api/chat or OpenAI /v1/chat/completions")
    p.add_argument("--ollama-url", default="http://192.168.68.94:11434", help="Base URL for ollama backend")
    p.add_argument("--openai-base-url", default="https://api.together.xyz", help="Base URL for openai backend (Together AI by default)")
    p.add_argument("--openai-api-key", default=None, help="API key for openai backend; falls back to TOGETHER_API_KEY from .env/openai.env if not set")
    p.add_argument("--mcp-command", required=True, help="Path to the nebula MCP binary (the runner adds the 'mcp' subcommand automatically)")
    p.add_argument("--task-dir", required=True, help="Working directory for bash commands and the MCP server")
    p.add_argument("--prompt-file", help="File containing the user prompt")
    p.add_argument("--prompt", help="Inline user prompt (alternative to --prompt-file)")
    p.add_argument("--verify", help="Verification command to run AFTER the agent loop ends; result tabulated in summary")
    p.add_argument("--output", required=True, help="JSONL log path")
    p.add_argument("--max-turns", type=int, default=30)
    p.add_argument("--num-ctx", type=int, default=32768)
    p.add_argument("--temperature", type=float, default=0.1)
    p.add_argument("--neb-ollama-url", default="http://192.168.68.94:11434", help="Where the MCP server should reach the translator (passed via NEBULA_OLLAMA_URL)")
    p.add_argument("--neb-ollama-model", default="nebula-8lang-7b:latest")
    args = p.parse_args()

    if args.prompt_file:
        user_prompt = Path(args.prompt_file).read_text()
    elif args.prompt:
        user_prompt = args.prompt
    else:
        sys.exit("--prompt or --prompt-file required")

    cmd = [args.mcp_command, "mcp"]

    mcp_env = {
        "NEBULA_OLLAMA_URL": args.neb_ollama_url,
        "NEBULA_OLLAMA_MODEL": args.neb_ollama_model,
    }

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    print(f"=== Host agent runner ===", flush=True)
    print(f"  backend:      {args.backend}", flush=True)
    print(f"  model:        {args.model}", flush=True)
    if args.backend == "ollama":
        print(f"  ollama_url:   {args.ollama_url}", flush=True)
    else:
        print(f"  openai_url:   {args.openai_base_url}", flush=True)
    print(f"  task_dir:     {args.task_dir}", flush=True)
    print(f"  mcp:          {' '.join(cmd)}", flush=True)
    print(f"  output:       {out_path}", flush=True)
    print(f"  max_turns:    {args.max_turns}", flush=True)
    print(flush=True)

    mcp = MCPClient(cmd, env=mcp_env)
    mcp.start(cwd=args.task_dir)

    if args.backend == "ollama":
        ollama = OllamaClient(args.ollama_url, args.model, num_ctx=args.num_ctx, temperature=args.temperature)
    else:
        api_key = args.openai_api_key
        if not api_key:
            for env_file in (Path(".env"), Path("openai.env")):
                if env_file.exists():
                    for line in env_file.read_text().splitlines():
                        if line.startswith("TOGETHER_API_KEY="):
                            api_key = line.split("=", 1)[1].strip().strip('"').strip("'")
                            break
                if api_key:
                    break
        if not api_key:
            sys.exit("--backend openai requires --openai-api-key or TOGETHER_API_KEY in env/.env")
        ollama = OpenAIClient(args.openai_base_url, args.model, api_key=api_key, temperature=args.temperature)

    log_fp = open(out_path, "w")
    log_event(log_fp, {"type": "config", "model": args.model, "task_dir": args.task_dir, "max_turns": args.max_turns})

    try:
        log_event(log_fp, {"type": "user", "content": user_prompt})
        final_text, metrics = run_agent_loop(
            ollama, mcp, user_prompt,
            bash_cwd=args.task_dir,
            max_turns=args.max_turns,
            log_fp=log_fp,
        )
    finally:
        mcp.stop()

    # Run verification command outside the loop for the audit trail
    verify_result = None
    if args.verify:
        ok, output = run_bash(args.verify, args.task_dir)
        verify_result = {"command": args.verify, "ok": ok, "output": output[:4000]}

    summary = {
        "type": "summary",
        "stopped_reason": metrics["stopped_reason"],
        "turns": metrics["turns"],
        "tool_calls": metrics["tool_calls"],
        "tool_calls_by_name": metrics["tool_calls_by_name"],
        "input_tokens": metrics["input_tokens"],
        "output_tokens": metrics["output_tokens"],
        "total_tokens": metrics["input_tokens"] + metrics["output_tokens"],
        "duration_ms": metrics["duration_ms"],
        "nudges_replace_all": metrics.get("nudges_replace_all", 0),
        "nudges_stuck": metrics.get("nudges_stuck", 0),
        "final_text": final_text[:1000],
        "verify": verify_result,
    }
    log_event(log_fp, summary)
    log_fp.close()

    print(flush=True)
    print(f"=== Summary ===", flush=True)
    print(f"  stopped:        {summary['stopped_reason']}", flush=True)
    print(f"  turns:          {summary['turns']}", flush=True)
    print(f"  tool_calls:     {summary['tool_calls']}  {summary['tool_calls_by_name']}", flush=True)
    print(f"  input_tokens:   {summary['input_tokens']}", flush=True)
    print(f"  output_tokens:  {summary['output_tokens']}", flush=True)
    print(f"  duration:       {summary['duration_ms']/1000:.1f}s", flush=True)
    if summary.get('nudges_replace_all') or summary.get('nudges_stuck'):
        print(f"  nudges:         replace_all={summary['nudges_replace_all']}  stuck={summary['nudges_stuck']}", flush=True)
    if verify_result:
        print(f"  verify:         {'PASS' if verify_result['ok'] else 'FAIL'} ({verify_result['command']})", flush=True)
        print(f"  verify_tail:    {verify_result['output'][-300:]}", flush=True)
    print(flush=True)
    print(f"Log written to {out_path}", flush=True)


if __name__ == "__main__":
    main()
