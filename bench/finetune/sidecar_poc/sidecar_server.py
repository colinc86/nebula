#!/usr/bin/env python3
"""Phase 20 sidecar HTTP service.

Receives `(original_native, nebula_old_str, nebula_new_str, lang)` from the
Nebula MCP and returns `(native_old_str, native_new_str)` — the native-form
snippet pair that the MCP can then byte-replace into the file.

Backend: Together AI Qwen2.5-7B-Instruct (defaults to the fine-tuned sidecar
model once available, falls back to the stock 7B for plumbing tests).

Usage:
    NEBULA_SIDECAR_MODEL=Qwen/Qwen2.5-7B-Instruct \\
    .venv/bin/python3 bench/finetune/sidecar_poc/sidecar_server.py [PORT]

The MCP picks this up automatically when started with NEBULA_FULL_NEBULA=1.
Endpoint: POST http://localhost:PORT/correct  (default PORT 8765)
"""

from __future__ import annotations
import json
import os
import re
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
ENV_FILE = REPO / ".env"

DEFAULT_PORT = int(os.environ.get("NEBULA_SIDECAR_PORT", "8765"))
# NEBULA_SIDECAR_BACKEND chooses inference backend:
#   "together"  → call the Together API (uses NEBULA_SIDECAR_MODEL as model id)
#   "ollama"    → call a local Ollama /api/chat (uses NEBULA_SIDECAR_MODEL as
#                 the Ollama tag, NEBULA_SIDECAR_OLLAMA_URL as endpoint)
DEFAULT_BACKEND = os.environ.get("NEBULA_SIDECAR_BACKEND", "together")
# Default model depends on backend. For Together we default to the 480B
# teacher because Qwen2.5-7B-Instruct is fine-tunable but not serverless;
# the production deployment uses the local Ollama backend with the
# fine-tuned 7B served on the GPU box.
DEFAULT_MODEL = os.environ.get(
    "NEBULA_SIDECAR_MODEL",
    "nebula-sidecar-7b" if DEFAULT_BACKEND == "ollama" else "Qwen/Qwen3-Coder-480B-A35B-Instruct-FP8",
)
DEFAULT_OLLAMA_URL = os.environ.get("NEBULA_SIDECAR_OLLAMA_URL", "http://192.168.68.94:11434")

sys.path.insert(0, str(REPO / ".venv" / "lib" / "python3.13" / "site-packages"))
sys.path.insert(0, str(REPO / "bench" / "finetune" / "qwen3_coder_30b"))
from distill import _load_together_key, _together_chat  # type: ignore


SYSTEM_PROMPT = """You translate Nebula IL diff snippets into native-language diff snippets, using the original native source as a style anchor.

You receive:
- ORIGINAL: the full native-language source file. This is the ground truth for formatting, identifier casing, comments, and every byte that won't change.
- NEBULA_OLD: a Nebula-IL representation of the snippet the inner agent wants to replace.
- NEBULA_NEW: a Nebula-IL representation of what to replace it with.

Your job: produce two strings, NATIVE_OLD and NATIVE_NEW, such that:
1. NATIVE_OLD appears verbatim in ORIGINAL (byte-for-byte). It is the contiguous slice of bytes whose semantics match NEBULA_OLD.
2. NATIVE_NEW is the corresponding native-language replacement, written in the exact style of ORIGINAL (CamelCase, native braces/loops, tabs vs spaces, comment style).
3. Replacing NATIVE_OLD with NATIVE_NEW in ORIGINAL produces a file equivalent to one where the Nebula diff has been applied.

Output STRICTLY this JSON object on a single line, with no markdown fence and no commentary:

{"native_old_str": "...", "native_new_str": "..."}

Use proper JSON string escaping for newlines (\\n), tabs (\\t), backslashes, and quotes. Do not output anything outside the braces."""


USER_TEMPLATE = """Language: {lang}

ORIGINAL ({lang}):
```{lang}
{original}
```

NEBULA_OLD:
```
{nebula_old}
```

NEBULA_NEW:
```
{nebula_new}
```

Output the JSON now:"""


def call_together(api_key: str, model: str, original: str, neb_old: str, neb_new: str, lang: str) -> dict:
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": USER_TEMPLATE.format(
            lang=lang, original=original, nebula_old=neb_old, nebula_new=neb_new,
        )},
    ]
    import distill  # type: ignore
    saved = distill.TOGETHER_MODEL
    try:
        distill.TOGETHER_MODEL = model
        resp = _together_chat(api_key, messages, tools=[])
    finally:
        distill.TOGETHER_MODEL = saved
    text = (resp.get("choices", [{}])[0].get("message", {}) or {}).get("content", "") or ""
    return {"text": text, "usage": resp.get("usage", {}) or {}}


def call_ollama(ollama_url: str, model: str, original: str, neb_old: str, neb_new: str, lang: str) -> dict:
    """POST to a local Ollama /api/chat. Returns {text, usage}.

    Phase 20 detail: we set keep_alive='15s' so the sidecar model unloads from
    VRAM 15 seconds after each call. The benchmark inner agent (qwen3-coder:30b)
    needs ~18 GB and the Q4 sidecar needs ~5 GB; together they run right at the
    24 GB ceiling. Letting Ollama swap the sidecar out between patch_code calls
    keeps the inner agent fully on GPU during its own (much longer) generation
    runs.
    """
    import json as _json
    import urllib.request
    body = _json.dumps({
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": USER_TEMPLATE.format(
                lang=lang, original=original, nebula_old=neb_old, nebula_new=neb_new,
            )},
        ],
        "stream": False,
        "keep_alive": "15s",
        "options": {"temperature": 0.05, "num_predict": 1024},
    }).encode()
    req = urllib.request.Request(
        f"{ollama_url.rstrip('/')}/api/chat",
        data=body, headers={"Content-Type": "application/json"}, method="POST",
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        data = _json.loads(resp.read())
    text = (data.get("message") or {}).get("content", "") or ""
    return {
        "text": text,
        "usage": {
            "prompt_tokens": data.get("prompt_eval_count", 0),
            "completion_tokens": data.get("eval_count", 0),
        },
    }


def call_model(server, original: str, neb_old: str, neb_new: str, lang: str) -> dict:
    if server.backend == "ollama":
        return call_ollama(server.ollama_url, server.model, original, neb_old, neb_new, lang)
    return call_together(server.api_key, server.model, original, neb_old, neb_new, lang)


def parse_response(text: str) -> tuple[str, str] | None:
    """Pull the JSON object out of the response. The response can be:
      (a) a bare JSON object (model followed instructions),
      (b) a JSON object wrapped in a ```json fence,
      (c) JSON with leading/trailing prose.

    The naive regex `\\{...\\}` fails because the JSON *values* contain `{`
    and `}` from native function bodies. We use Python's `json.JSONDecoder`
    in raw_decode mode, which finds the first valid JSON object even when
    surrounded by other text."""
    text = text.strip()
    # Strip a leading ```json or ``` fence if present.
    fence_m = re.match(r"^```(?:json)?\s*\n(.*?)\n```\s*$", text, re.DOTALL)
    if fence_m:
        text = fence_m.group(1).strip()
    # Find the first '{' and try raw_decode from there. If that fails, scan
    # forward to the next '{' until we find a valid JSON object.
    decoder = json.JSONDecoder()
    pos = 0
    while True:
        idx = text.find("{", pos)
        if idx < 0:
            return None
        try:
            obj, _ = decoder.raw_decode(text[idx:])
        except json.JSONDecodeError:
            pos = idx + 1
            continue
        if isinstance(obj, dict) and "native_old_str" in obj and "native_new_str" in obj:
            return obj["native_old_str"], obj["native_new_str"]
        pos = idx + 1


class Handler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        sys.stderr.write(f"[sidecar] {format % args}\n")

    def do_POST(self):
        if self.path not in ("/correct", "/correct/"):
            self.send_error(404, "Use POST /correct")
            return
        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length)
        try:
            req = json.loads(body)
        except json.JSONDecodeError as e:
            self._json_error(400, f"invalid JSON: {e}")
            return

        original = req.get("original_native", "")
        neb_old = req.get("nebula_old_str", "")
        neb_new = req.get("nebula_new_str", "")
        lang = req.get("lang", "go")

        if not original or not neb_old:
            self._json_error(400, "missing original_native or nebula_old_str")
            return

        try:
            r = call_model(self.server, original, neb_old, neb_new, lang)
        except Exception as e:
            self._json_error(500, f"model call failed: {e}")
            return

        parsed = parse_response(r["text"])
        if not parsed:
            self._json_error(502, f"sidecar response unparseable: {r['text'][:300]}")
            return
        native_old, native_new = parsed
        self._json_ok({"native_old_str": native_old, "native_new_str": native_new,
                       "model": self.server.model, "tokens": r["usage"]})

    def _json_ok(self, payload: dict):
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _json_error(self, code: int, msg: str):
        body = json.dumps({"error": msg}).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_PORT
    backend = DEFAULT_BACKEND
    if backend == "together":
        api_key = _load_together_key()
        if not api_key:
            print("TOGETHER_API_KEY not set", file=sys.stderr); sys.exit(1)
    else:
        api_key = ""
    print(f"Sidecar listening on http://localhost:{port}/correct  backend={backend}  model={DEFAULT_MODEL}", flush=True)
    if backend == "ollama":
        print(f"  ollama_url={DEFAULT_OLLAMA_URL}", flush=True)
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    server.api_key = api_key
    server.backend = backend
    server.model = DEFAULT_MODEL
    server.ollama_url = DEFAULT_OLLAMA_URL
    server.serve_forever()


if __name__ == "__main__":
    main()
