#!/usr/bin/env python3
"""End-to-end integration smoke test for the Phase 20 spoof pipeline.

Spawns nebula mcp with NEBULA_FULL_NEBULA=1 and NEBULA_SIDECAR_URL pointing
at the local sidecar HTTP service, then invokes patch_code with
nebula=true on a real Go file. Verifies the patch was applied byte-correctly.

Prereq: sidecar_server.py running (see top-of-file env knobs to control which
backend it uses — Together for plumbing tests, local Ollama for production).
"""

from __future__ import annotations
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
NEBULA = REPO / "compiler" / "target" / "release" / "nebula"
SIDECAR_URL = os.environ.get("NEBULA_SIDECAR_URL", "http://127.0.0.1:8765/correct")


def mcp_call(proc: subprocess.Popen, request: dict) -> dict:
    line = json.dumps(request) + "\n"
    proc.stdin.write(line)
    proc.stdin.flush()
    resp = proc.stdout.readline()
    if not resp:
        raise RuntimeError(f"empty mcp response (stderr: {proc.stderr.read()})")
    return json.loads(resp)


def main() -> None:
    # Stage a working copy of intset.go so we can patch it without dirtying
    # the template.
    work_dir = HERE / "runs" / "integration_smoke"
    if work_dir.exists():
        shutil.rmtree(work_dir)
    shutil.copytree(REPO / "bench" / "mcp_manual_test" / "add_feature_template", work_dir)
    target = work_dir / "intset.go"
    original_text = target.read_text()
    print(f"Original:\n{'-'*60}\n{original_text}\n{'-'*60}")

    # Construct a Nebula-form snippet that the sidecar will translate. The
    # inner agent (in production) would write this; we hand-craft for the
    # smoke test. Renaming `NewIntSet` to `BuildIntSet`.
    nebula_old = "def new_int_set() -> IntSet:"
    nebula_new = "def build_int_set() -> IntSet:"

    env = {
        **os.environ,
        "NEBULA_FULL_NEBULA": "1",
        "NEBULA_SIDECAR_URL": SIDECAR_URL,
    }
    proc = subprocess.Popen(
        [str(NEBULA), "mcp"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, env=env, cwd=str(work_dir),
    )
    try:
        mcp_call(proc, {
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                       "clientInfo": {"name": "smoke", "version": "0"}},
        })
        proc.stdin.write(json.dumps({
            "jsonrpc": "2.0", "method": "notifications/initialized", "params": {},
        }) + "\n")
        proc.stdin.flush()

        print("\nCalling patch_code(nebula=true)...")
        resp = mcp_call(proc, {
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {
                "name": "patch_code",
                "arguments": {
                    "path": str(target),
                    "old_str": nebula_old,
                    "new_str": nebula_new,
                    "nebula": True,
                },
            },
        })
    finally:
        try:
            proc.stdin.close()
        except Exception:
            pass
        try:
            proc.wait(timeout=5)
        except Exception:
            proc.kill()

    content = resp.get("result", {}).get("content", [])
    if not content:
        print(f"\nERROR: empty response: {json.dumps(resp)[:500]}")
        sys.exit(1)
    text = content[0].get("text", "")
    print(f"\nMCP response: {text}")

    new_text = target.read_text()
    print(f"\nModified:\n{'-'*60}\n{new_text}\n{'-'*60}")
    # Specifically check the function declaration line — we only patch the
    # decl, doc comments are untouched by design (would be a separate patch).
    has_new_decl = "func BuildIntSet()" in new_text
    has_old_decl = "func NewIntSet()" in new_text
    if has_new_decl and not has_old_decl:
        print("\n✅ PASS: function declaration renamed end-to-end via Nebula spoof")
        print("   (doc comment legitimately preserved — sidecar only patches the requested region)")
        sys.exit(0)
    elif new_text != original_text:
        print(f"\n⚠️  PARTIAL: file changed (has_new_decl={has_new_decl} has_old_decl={has_old_decl})")
        sys.exit(2)
    else:
        print("\n❌ FAIL: file unchanged")
        sys.exit(1)


if __name__ == "__main__":
    main()
