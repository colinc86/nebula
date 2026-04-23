"""Thin wrapper over the nebula compiler binary.

Exposes `to_nebula(src, lang_ext)`, `to_native(nebula, lang_ext)`, and
`check(nebula)` as pure Python calls that handle tempfile management,
timeout, and stderr capture.
"""
from __future__ import annotations
import os, subprocess, tempfile
from pathlib import Path
from typing import Optional

from config import NEBULA_BIN, LANG_EXT


DEFAULT_TIMEOUT = 15.0


def _run(args: list[str], input_text: Optional[str] = None, timeout: float = DEFAULT_TIMEOUT) -> tuple[int, str, str]:
    """Run nebula CLI. Returns (rc, stdout, stderr)."""
    p = subprocess.run(
        [str(NEBULA_BIN), *args],
        input=input_text,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    return p.returncode, p.stdout, p.stderr


def _with_tempfile(suffix: str, content: str):
    """Context-ish helper: write content to a tempfile, yield its path, unlink."""
    fd, path = tempfile.mkstemp(suffix=suffix, text=True)
    try:
        with os.fdopen(fd, "w") as f:
            f.write(content)
        return path
    except Exception:
        os.unlink(path)
        raise


def to_nebula(source: str, lang: str) -> Optional[str]:
    """Convert native source to Nebula. Returns None on any error."""
    ext = LANG_EXT.get(lang)
    if ext is None:
        return None
    tmp = _with_tempfile(f".{ext}", source)
    try:
        rc, out, err = _run([f"from-{ext}", tmp])
        if rc != 0 or not out.strip():
            return None
        return out
    except Exception:
        return None
    finally:
        os.unlink(tmp)


def to_native(nebula: str, target_lang: str) -> Optional[str]:
    """Convert Nebula to target native. Returns None on any error.

    Uses `nebula to-<ext>` with a .neb tempfile.
    """
    ext = LANG_EXT.get(target_lang)
    if ext is None:
        return None
    tmp = _with_tempfile(".neb", nebula)
    try:
        rc, out, err = _run([f"to-{ext}", tmp])
        if rc != 0 or not out.strip():
            return None
        return out
    except Exception:
        return None
    finally:
        os.unlink(tmp)


def check(nebula: str) -> bool:
    """Parse-check. Returns True if the snippet parses."""
    tmp = _with_tempfile(".neb", nebula)
    try:
        rc, _out, _err = _run(["check", tmp])
        return rc == 0
    except Exception:
        return False
    finally:
        os.unlink(tmp)


if __name__ == "__main__":
    # Smoke test.
    src = 'def hello(name):\n    return "hi " + name\n'
    n = to_nebula(src, "python")
    print("nebula form:")
    print(n)
    print("check:", check(n))
    print("back to go:")
    print(to_native(n, "go"))
