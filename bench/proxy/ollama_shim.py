"""Tiny Ollama-API → OpenAI-API shim for the Nebula delegate.

The delegate in compiler/src/mcp_delegate.rs expects an Ollama instance on
`NEBULA_DELEGATE_OLLAMA_URL` with `/api/tags` and `/api/chat` endpoints.
This shim reimplements those endpoints against OpenAI's chat-completions
API so the delegate can route to our FT 4.1 model without touching the
Rust code.

Endpoints:
  GET  /api/tags   — returns the mapped model name so is_available() passes
  POST /api/chat   — converts Ollama chat request → OpenAI, forwards,
                     converts response back to Ollama's `{message:{...}, done:true}` shape

Launch: `.venv/bin/python3 bench/proxy/ollama_shim.py`
Listens on 127.0.0.1:11434 (default Ollama port).

Point the delegate at it:
  export NEBULA_DELEGATE_OLLAMA_URL=http://localhost:11434
  export NEBULA_DELEGATE_MODEL=nebula-prod-v2
"""
from __future__ import annotations
import json
import os
import sys
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# Load .env for OPENAI_API_KEY.
REPO = Path(__file__).resolve().parent.parent.parent
for line in (REPO / ".env").read_text().splitlines():
    if line.startswith("OPENAI_API_KEY="):
        os.environ["OPENAI_API_KEY"] = line.split("=", 1)[1].strip().strip('"').strip("'")
        break

from openai import OpenAI  # noqa: E402

# Incoming Ollama model names → OpenAI FT checkpoint IDs. Unknown names fall
# through to DEFAULT_TARGET. Lets the corrector pick between prod-v2 (4.1) and
# v3-mini (4o-mini) at call time via `NEBULA_CORRECTOR_MODEL=nebula-v3-mini`.
MODEL_ROUTES = {
    "nebula-prod-v2":
        "ft:gpt-4.1-2025-04-14:personal:nebulahost-prod-v2:DWvnbtaT",
    "nebula-v3-mini":
        "ft:gpt-4o-mini-2024-07-18:personal:nebulahost-pilot-v3:DWlP12V3",
    "nebula-pilot-v3":
        "ft:gpt-4o-mini-2024-07-18:personal:nebulahost-pilot-v3:DWlP12V3",
}
DEFAULT_TARGET = os.environ.get(
    "NEBULA_SHIM_TARGET_MODEL",
    MODEL_ROUTES["nebula-prod-v2"],
)


def resolve_target(incoming_model: str) -> str:
    """Map the Ollama-level model name (e.g. `nebula-v3-mini`) to its FT id.
    Unknown names route to DEFAULT_TARGET (prod-v2 by default)."""
    return MODEL_ROUTES.get(incoming_model, DEFAULT_TARGET)

client = OpenAI()


def ollama_to_openai_tools(ollama_tools):
    """Ollama exposes `[{type:function, function:{name,description,parameters}}]`
    — same shape as OpenAI. Pass through."""
    return ollama_tools or None


def openai_to_ollama_toolcalls(openai_calls):
    """OpenAI: arguments is a JSON string. Ollama: arguments is a dict."""
    out = []
    for tc in openai_calls or []:
        fn = getattr(tc, "function", None) or tc.get("function", {})
        name = getattr(fn, "name", None) or fn.get("name")
        args_raw = getattr(fn, "arguments", None) or fn.get("arguments", "")
        try:
            args = json.loads(args_raw) if isinstance(args_raw, str) else args_raw
        except Exception:
            args = {}
        out.append({"function": {"name": name, "arguments": args}})
    return out


class OllamaShimHandler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        sys.stderr.write(f"[shim] {self.address_string()} - " + (fmt % args) + "\n")

    def _json(self, code, payload):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.startswith("/api/tags"):
            self._json(200, {
                "models": [
                    {"name": name, "model": name, "size": 0,
                     "digest": "shim", "details": {}}
                    for name in MODEL_ROUTES
                ]
            })
            return
        self.send_error(404)

    def do_POST(self):
        if not self.path.startswith("/api/chat"):
            self.send_error(404)
            return
        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length)
        try:
            req = json.loads(body)
        except Exception as e:
            self._json(400, {"error": f"bad JSON: {e}"})
            return

        messages = req.get("messages", [])
        tools = ollama_to_openai_tools(req.get("tools"))
        # Ollama "options" carry temp/num_ctx. Map what we can.
        opts = req.get("options") or {}
        temperature = opts.get("temperature", 0.2)
        # max_tokens unset → let OpenAI decide. num_predict caps output.
        max_tokens = opts.get("num_predict") or req.get("num_predict") or 2048

        # OpenAI messages need tool_calls in a specific shape; Ollama already
        # emits {"role":"assistant","content":"","tool_calls":[{function:...}]}
        # which we need to adapt for OpenAI API (arguments as JSON string).
        openai_messages = []
        for m in messages:
            if m.get("role") == "assistant" and m.get("tool_calls"):
                calls = []
                for i, tc in enumerate(m["tool_calls"]):
                    fn = tc.get("function", {})
                    args = fn.get("arguments", {})
                    if isinstance(args, dict):
                        args = json.dumps(args)
                    calls.append({
                        "id": tc.get("id") or f"call_{i}",
                        "type": "function",
                        "function": {"name": fn.get("name", ""), "arguments": args},
                    })
                openai_messages.append({
                    "role": "assistant",
                    "content": m.get("content") or None,
                    "tool_calls": calls,
                })
            elif m.get("role") == "tool":
                openai_messages.append({
                    "role": "tool",
                    "tool_call_id": m.get("tool_call_id") or "call_0",
                    "content": m.get("content", ""),
                })
            else:
                openai_messages.append({
                    "role": m.get("role", "user"),
                    "content": m.get("content", ""),
                })

        incoming_model = req.get("model", "nebula-prod-v2")
        target = resolve_target(incoming_model)
        sys.stderr.write(f"[shim] routing {incoming_model} → {target}\n")
        try:
            kwargs = dict(
                model=target,
                messages=openai_messages,
                temperature=temperature,
                max_tokens=max_tokens,
            )
            if tools:
                kwargs["tools"] = tools
                kwargs["tool_choice"] = "auto"
            resp = client.chat.completions.create(**kwargs)
        except Exception as e:
            sys.stderr.write(f"[shim] OpenAI error: {e}\n")
            self._json(500, {"error": str(e)})
            return

        choice = resp.choices[0]
        msg = choice.message
        ollama_msg = {
            "role": "assistant",
            "content": msg.content or "",
        }
        if msg.tool_calls:
            ollama_msg["tool_calls"] = openai_to_ollama_toolcalls(msg.tool_calls)

        out = {
            "model": req.get("model", "nebula-prod-v2"),
            "message": ollama_msg,
            "done": True,
            "done_reason": choice.finish_reason or "stop",
            # Usage fields — Ollama-ish names. The delegate reads these for
            # metrics via ureq::post(...) response parsing.
            "prompt_eval_count": getattr(resp.usage, "prompt_tokens", 0) if resp.usage else 0,
            "eval_count": getattr(resp.usage, "completion_tokens", 0) if resp.usage else 0,
            "total_duration": 0,
            "load_duration": 0,
            "prompt_eval_duration": 0,
            "eval_duration": 0,
        }
        self._json(200, out)


def main():
    host = "127.0.0.1"
    # Default to 11435 to avoid clashing with any real Ollama on 11434.
    port = int(os.environ.get("NEBULA_SHIM_PORT", "11435"))
    srv = ThreadingHTTPServer((host, port), OllamaShimHandler)
    sys.stderr.write(f"[shim] listening on {host}:{port}\n")
    for name, target in MODEL_ROUTES.items():
        sys.stderr.write(f"[shim]   {name} → {target}\n")
    sys.stderr.write(f"[shim]   (default) → {DEFAULT_TARGET}\n")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        srv.server_close()


if __name__ == "__main__":
    main()
