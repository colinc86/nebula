#!/usr/bin/env python3
"""Local HTTP proxy that translates Ollama /api/chat requests into Together
/v1/chat/completions requests and translates the responses back.

This lets the Nebula MCP delegate (which speaks Ollama protocol) evaluate
against a Together-hosted dedicated endpoint without downloading/converting
the model locally.

Usage:
    TOGETHER_MODEL=colinc86_4aa1/Model-Name-hash \
    python3 ollama_to_together_proxy.py [PORT]

The MCP should point NEBULA_DELEGATE_OLLAMA_URL at http://127.0.0.1:PORT.
"""

from __future__ import annotations
import json
import os
import sys
import urllib.request
import urllib.error
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
ENV_FILE = REPO / ".env"

DEFAULT_PORT = int(os.environ.get("PROXY_PORT", "11436"))
TOGETHER_URL = "https://api.together.xyz/v1/chat/completions"
TOGETHER_MODEL = os.environ.get("TOGETHER_MODEL", "")


sys.path.insert(0, str(REPO / ".venv" / "lib" / "python3.13" / "site-packages"))

def load_api_key() -> str:
    if "TOGETHER_API_KEY" in os.environ:
        return os.environ["TOGETHER_API_KEY"]
    for line in ENV_FILE.read_text().splitlines():
        if line.startswith("TOGETHER_API_KEY="):
            return line.split("=", 1)[1].strip().strip('"').strip("'")
    return ""


def ollama_to_openai(ollama_body: dict) -> dict:
    """Convert Ollama /api/chat request body → OpenAI /v1/chat/completions."""
    opts = ollama_body.get("options", {})
    openai = {
        "model": TOGETHER_MODEL or ollama_body.get("model", ""),
        "messages": ollama_body.get("messages", []),
        "stream": False,
        "temperature": opts.get("temperature", 0.2),
        "max_tokens": opts.get("num_predict", 2048),
    }
    tools = ollama_body.get("tools", [])
    if tools:
        openai["tools"] = tools
        openai["tool_choice"] = "auto"
    return openai


def openai_to_ollama(openai_resp: dict) -> dict:
    """Convert OpenAI /v1/chat/completions response → Ollama /api/chat."""
    choice = (openai_resp.get("choices") or [{}])[0]
    msg = choice.get("message") or {}
    usage = openai_resp.get("usage") or {}

    ollama = {
        "model": openai_resp.get("model", ""),
        "message": msg,
        "done": True,
        "done_reason": choice.get("finish_reason", "stop"),
        "prompt_eval_count": usage.get("prompt_tokens", 0),
        "eval_count": usage.get("completion_tokens", 0),
        # Ollama normally has timing fields; we fake them
        "prompt_eval_duration": 0,
        "eval_duration": 0,
        "total_duration": 0,
    }
    return ollama


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        sys.stderr.write(f"[proxy] {fmt % args}\n")

    def do_GET(self):
        if self.path in ("/api/tags", "/api/tags/"):
            # Ollama model list — return fake entry so MCP's is_available() passes
            body = json.dumps({"models": [{"name": TOGETHER_MODEL, "model": TOGETHER_MODEL}]}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body)
            return
        if self.path in ("/api/ps", "/api/ps/"):
            body = json.dumps({"models": []}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_error(404)

    def do_POST(self):
        if self.path not in ("/api/chat", "/api/chat/", "/api/generate", "/api/generate/"):
            self.send_error(404, f"Unknown path: {self.path}")
            return

        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length)
        try:
            ollama_body = json.loads(raw)
        except json.JSONDecodeError as e:
            self._json_error(400, f"bad JSON: {e}")
            return

        # Handle keep_alive=0 (unload) silently
        if ollama_body.get("keep_alive") == 0 or ollama_body.get("keep_alive") == "0":
            body = json.dumps({"model": ollama_body.get("model", ""), "done": True, "done_reason": "unload"}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body)
            return

        openai_body = ollama_to_openai(ollama_body)
        # Use the Together SDK's httpx client — raw urllib gets 403'd by
        # Cloudflare's WAF. The SDK adds the right User-Agent / HTTP2 headers.
        try:
            kwargs = {
                "model": openai_body["model"],
                "messages": openai_body["messages"],
                "max_tokens": openai_body.get("max_tokens", 2048),
                "temperature": openai_body.get("temperature", 0.2),
                "stream": False,
            }
            if openai_body.get("tools"):
                kwargs["tools"] = openai_body["tools"]
                kwargs["tool_choice"] = "auto"
            resp_obj = self.server.together_client.chat.completions.create(**kwargs)
            openai_resp = json.loads(resp_obj.model_dump_json())
        except Exception as e:
            self._json_error(502, f"Together SDK error: {e}")
            return

        ollama_resp = openai_to_ollama(openai_resp)
        body = json.dumps(ollama_resp).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(body)

    def _json_error(self, code: int, msg: str):
        body = json.dumps({"error": msg}).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(body)


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_PORT
    api_key = load_api_key()
    if not api_key:
        print("TOGETHER_API_KEY not set", file=sys.stderr)
        sys.exit(1)
    if not TOGETHER_MODEL:
        print("TOGETHER_MODEL not set", file=sys.stderr)
        sys.exit(1)
    print(f"Ollama→Together proxy on http://127.0.0.1:{port}")
    print(f"  model: {TOGETHER_MODEL}")
    print(f"  target: {TOGETHER_URL}", flush=True)
    from together import Together
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    server.api_key = api_key
    server.together_client = Together(api_key=api_key)
    server.serve_forever()


if __name__ == "__main__":
    main()
