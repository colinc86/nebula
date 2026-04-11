#!/usr/bin/env python3
"""Nebula 0.3 transform: convert brace-form Nebula → indent-form Nebula.

This is a throwaway text transform for the static compression measurement.
It does NOT preserve executable Nebula — it produces a hypothetical
indent-based form so we can measure its cl100k_base footprint vs the
current brace form, Python, and SimPy/Token Sugar.

Transforms applied:
  1. Drop `{` and `}` block delimiters
  2. Drop `lt ` / `mt ` prefixes (Python-style bare assignment)
  3. Replace lossy method aliases with the Python originals:
     `.psh(` → `.append(`, `.jn(` → `.join(`, ` srt(` → ` sorted(`
  4. Re-emit block headers with `:` (Python style)
  5. Normalize 4-space indentation

The transform is line-oriented and preserves string literal contents.
"""

from __future__ import annotations
import re


def _walk_outside_strings(line: str, callback):
    """Run `callback(segment)` over outside-string segments of `line`,
    reassembling the result with string literals copied verbatim."""
    out = []
    i = 0
    buf = ""
    n = len(line)
    while i < n:
        c = line[i]
        if c == '"' or c == "'":
            # Flush buffer, copy string literal.
            out.append(callback(buf))
            buf = ""
            q = c
            lit = c
            i += 1
            while i < n:
                ch = line[i]
                lit += ch
                if ch == "\\" and i + 1 < n:
                    lit += line[i + 1]
                    i += 2
                    continue
                i += 1
                if ch == q:
                    break
            out.append(lit)
            continue
        buf += c
        i += 1
    out.append(callback(buf))
    return "".join(out)


_METHOD_FIXES = [
    (".psh(", ".append("),
    (".jn(", ".join("),
    (".mp(", ".map("),
    (".fl(", ".filter("),
    (".rv(", ".reverse("),
    (".clr(", ".clear("),
    (".con(", ".contains("),
    (" srt(", " sorted("),
    (" ln(", " len("),
    (" rng(", " range("),
    (" enu(", " enumerate("),
    (" prt(", " print("),
]


def _rewrite_methods(segment: str) -> str:
    for old, new in _METHOD_FIXES:
        segment = segment.replace(old, new)
    return segment


def transform(source: str) -> str:
    """Convert brace-form Nebula source to indent-form Nebula 0.3.

    Strategy: parse brace nesting depth as we go. A `{` at end of line opens
    a block; a standalone `}` line closes it. `lt`/`mt` prefixes are
    stripped. Block headers are re-emitted with trailing `:`. Indent is
    re-derived from depth, always 4 spaces per level.
    """
    lines = source.splitlines()
    out: list[str] = []
    depth = 0

    for raw in lines:
        # Preserve blank lines.
        if not raw.strip():
            out.append("")
            continue

        # Detect `}` close: either a standalone `}` or `} el {` / `} el if`
        stripped = raw.strip()

        # Handle `} el {` / `} el if ... {` / `} el` chain continuation.
        # Nebula puts these on a single line; Python splits them.
        if stripped.startswith("}"):
            # Close one level first.
            depth = max(0, depth - 1)
            remainder = stripped[1:].lstrip()
            if not remainder:
                # pure `}` — closed, emit nothing
                continue
            # `} el { ...` or `} el if x { ...`
            if remainder.startswith("el"):
                # Strip trailing `{` if present.
                if remainder.rstrip().endswith("{"):
                    remainder = remainder.rstrip()[:-1].rstrip()
                # `el if x` → `elif x:`
                if remainder.startswith("el if"):
                    body = remainder[len("el if") :].strip()
                    out.append(_indent(depth) + f"elif {body}:")
                elif remainder == "el":
                    out.append(_indent(depth) + "else:")
                else:
                    # `el` followed by something unusual — just emit with colon
                    out.append(_indent(depth) + remainder.replace("el", "else", 1) + ":")
                depth += 1
                continue
            # `} ch e { ...` (catch block) — rare, keep as comment
            if remainder.startswith("ch"):
                if remainder.rstrip().endswith("{"):
                    remainder = remainder.rstrip()[:-1].rstrip()
                body = remainder[len("ch") :].strip()
                out.append(_indent(depth) + f"except Exception as {body or 'e'}:")
                depth += 1
                continue
            # Unknown trailing content — emit as-is minus the brace
            out.append(_indent(depth) + remainder)
            continue

        # Strip `lt ` / `mt ` prefix. These are Python-style bare assignment.
        body = stripped
        if body.startswith("lt "):
            body = body[3:]
        elif body.startswith("mt "):
            body = body[3:]

        # Fix lossy method aliases (outside string literals only).
        body = _walk_outside_strings(body, _rewrite_methods)

        # If the line ends in `{` — it's a block header. Strip the brace,
        # add `:` for Python-style block opener.
        if body.rstrip().endswith("{"):
            header = body.rstrip()[:-1].rstrip()
            # Some block headers already end with `:` for match arms etc.
            if not header.endswith(":"):
                header += ":"
            out.append(_indent(depth) + header)
            depth += 1
            continue

        out.append(_indent(depth) + body)

    return "\n".join(out) + "\n"


def _indent(depth: int) -> str:
    return "    " * depth


# ── Docstring / type-annotation stripper (for the "Nebula 0.3+strip" column) ──


_DOCSTRING_BLOCK = re.compile(
    r"(?:^\s*//[^\n]*\n)+",
    re.MULTILINE,
)
_COMMENT_LINE = re.compile(r"^\s*//[^\n]*$", re.MULTILINE)


def strip_docstrings(source: str) -> str:
    """Remove all `//` comment lines. In Nebula 0.3 transformed output,
    docstrings are already `// ...` comment blocks, so this removes them
    wholesale."""
    return re.sub(r"^\s*//[^\n]*\n", "", source, flags=re.MULTILINE)


_TYPE_ANN_PARAM = re.compile(r"(\w+)\s*:\s*[\w\[\], ]+?(?=[,)])")
_TYPE_ANN_RETURN = re.compile(r"\s*->\s*[\w\[\], ]+?(?=[:\n{])")


def strip_type_annotations(source: str) -> str:
    """Remove type annotations on parameters and return types.

    `fn f(x: Int, y: List[Int]) -> Int:` → `fn f(x, y):`
    """
    result_lines = []
    for line in source.splitlines():
        # Only act on lines that look like a function header.
        if line.lstrip().startswith("fn ") and "(" in line:
            # Strip param annotations and return types.
            new_line = _TYPE_ANN_RETURN.sub("", line)
            new_line = _TYPE_ANN_PARAM.sub(r"\1", new_line)
            result_lines.append(new_line)
        else:
            result_lines.append(line)
    return "\n".join(result_lines) + ("\n" if source.endswith("\n") else "")


if __name__ == "__main__":
    import sys
    src = sys.stdin.read()
    out = transform(src)
    if "--strip" in sys.argv:
        out = strip_docstrings(out)
        out = strip_type_annotations(out)
    sys.stdout.write(out)
