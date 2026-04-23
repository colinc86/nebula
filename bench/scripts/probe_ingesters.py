#!/usr/bin/env python3
"""
Four-phase round-trip scanner for Nebula ingesters + codegen.

For each probe file in compiler/tests/probe/<lang>/*.ext:
  Phase 1 — ingest: `nebula from-<lang> file.ext` → Nebula text.
  Phase 2 — grammar check: `nebula check` on the Nebula text.
  Phase 3 — round-trip codegen: `nebula to-<lang>` back to the source language.
  Phase 4 — native compile/parse: hand the emitted code to the native parser.

Also scans the Nebula output for anti-patterns: TODO leaks, bracket imbalance,
stray `fn():` mid-argument, empty function bodies, casing-variant duplicates.

Output rows use the schema requested in the auditing brief:
  {
    "file": "probe/go/closures.go",
    "lang": "go",
    "idiom": "closures",
    "phase1_from_nebula": "ok|err: <msg>",
    "phase2_nebula_check": "ok|err at L:C <msg>",
    "phase3_to_nebula": "ok|err: <msg>",   # (to_<lang> codegen)
    "phase4_native_compile": "ok|err: <msg>",
    "antipatterns": [...],
    "nebula_len": N,
    "severity": "critical|major|minor|clean"
  }

Catalog: bench/scripts/probe_catalog.json.
Idempotent — running twice produces the same catalog modulo ordering.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parents[2]
PROBE_DIR = ROOT / "compiler" / "tests" / "probe"
NEBULA_BIN = ROOT / "compiler" / "target" / "release" / "nebula"

LANG_BY_DIR = {
    "go": ("go", "go"),
    "python": ("python", "py"),
    "javascript": ("javascript", "js"),
    "typescript": ("typescript", "ts"),
    "rust": ("rust", "rs"),
    "swift": ("swift", "swift"),
    "kotlin": ("kotlin", "kt"),
    "c": ("c", "c"),
}

FROM_SUBCMD = {
    "go": "from-go",
    "python": "from-python",
    "javascript": "from-js",
    "typescript": "from-ts",
    "rust": "from-rs",
    "swift": "from-swift",
    "kotlin": "from-kt",
    "c": "from-c",
}

TO_SUBCMD = {
    "go": "to-go",
    "python": "to-python",
    "javascript": "to-js",
    "typescript": "to-ts",
    "rust": "to-rs",
    "swift": "to-swift",
    "kotlin": "to-kt",
    "c": "to-c",
}


# ------------------------------ data model ------------------------------


@dataclass
class ProbeRow:
    file: str
    lang: str
    idiom: str
    phase1_from_nebula: str = "ok"
    phase2_nebula_check: str = "ok"
    phase3_to_nebula: str = "ok"
    phase4_native_compile: str = "ok"
    antipatterns: list[str] = field(default_factory=list)
    nebula_len: int = 0
    severity: str = "clean"
    # extra diagnostic payload (not in the brief's row schema but useful)
    nebula_output: str = ""
    roundtrip_output: str = ""
    antipattern_detail: list[dict] = field(default_factory=list)


_TMP_PATH_RE = re.compile(r"/(?:var|private/var|tmp)/[^\s:]*?tmp[^\s:/\\]+")
_ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")


def scrub(s: str) -> str:
    """Remove temp-file paths and ANSI color codes so catalog is idempotent."""
    s = _ANSI_RE.sub("", s)
    s = _TMP_PATH_RE.sub("<tmp>", s)
    return s


def trunc(s: str, n: int = 240) -> str:
    s = scrub(s or "").strip().replace("\n", " | ")
    return s if len(s) <= n else s[:n] + "..."


def run(args: list[str], input_path: Optional[Path] = None,
        stdin_text: Optional[str] = None,
        cwd: Optional[Path] = None, env: Optional[dict] = None,
        timeout: int = 45) -> tuple[int, str, str]:
    cmd = list(args)
    if input_path is not None:
        cmd.append(str(input_path))
    try:
        p = subprocess.run(
            cmd,
            input=stdin_text if stdin_text is not None else None,
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=str(cwd) if cwd else None,
            env=env,
        )
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired:
        return 124, "", "timeout"
    except FileNotFoundError as e:
        return 127, "", f"not-found: {e}"


# ------------------------- anti-pattern scanners -------------------------


TOKEN_KEYWORDS = {
    "fn", "def", "rt", "return", "fo", "for", "lt", "let", "mt", "mut",
    "if", "elif", "else", "while", "loop", "break", "continue",
    "class", "struct", "impl", "trait", "enum", "interface",
    "and", "or", "not", "in", "is", "as", "match", "case",
    "try", "catch", "except", "finally", "throw", "raise",
    "async", "await", "yield", "with", "pass", "true", "false", "none",
    "True", "False", "None", "self", "Self", "import", "use", "module",
    "pub", "static", "const", "new", "delete", "sizeof", "typedef",
}


def _strip_line_comments_and_strings(text: str) -> str:
    # Very rough: drop # ... eol comments and "…"/'…' string contents.
    out = []
    in_str: Optional[str] = None
    escape = False
    in_line_comment = False
    for ch in text:
        if ch == "\n":
            in_line_comment = False
            if in_str in ('"', "'"):
                in_str = None
            out.append(ch)
            continue
        if in_line_comment:
            continue
        if escape:
            escape = False
            continue
        if in_str:
            if ch == "\\":
                escape = True
                continue
            if ch == in_str:
                in_str = None
            continue
        if ch == "#":
            in_line_comment = True
            continue
        if ch in ('"', "'"):
            in_str = ch
            continue
        out.append(ch)
    return "".join(out)


def balance_check(text: str) -> list[dict]:
    pairs = {"(": ")", "[": "]", "{": "}"}
    openers = set(pairs.keys())
    closers = {v: k for k, v in pairs.items()}

    stack: list[tuple[str, int]] = []
    in_str: Optional[str] = None
    escape = False
    line = 1
    in_line_comment = False
    findings: list[dict] = []
    for ch in text:
        if ch == "\n":
            line += 1
            in_line_comment = False
            if in_str in ('"', "'"):
                in_str = None
            continue
        if in_line_comment:
            continue
        if escape:
            escape = False
            continue
        if in_str:
            if ch == "\\":
                escape = True
                continue
            if ch == in_str:
                in_str = None
            continue
        if ch == "#":
            in_line_comment = True
            continue
        if ch in ('"', "'"):
            in_str = ch
            continue
        if ch in openers:
            stack.append((ch, line))
        elif ch in closers:
            if not stack:
                findings.append({"rule": "unbalanced_close",
                                 "message": f"unexpected '{ch}' at L{line}",
                                 "line": line})
                continue
            top, _ = stack.pop()
            if pairs[top] != ch:
                findings.append({"rule": "mismatched_bracket",
                                 "message": f"'{top}' closed by '{ch}' at L{line}",
                                 "line": line})
    for top, ln in stack:
        findings.append({"rule": "unclosed_open",
                         "message": f"'{top}' opened at L{ln} never closed",
                         "line": ln})
    return findings


TODO_RE = re.compile(r"(?:#|//|/\*)\s*TODO", re.IGNORECASE)


def todo_check(text: str) -> list[dict]:
    out = []
    for i, line in enumerate(text.splitlines(), 1):
        if TODO_RE.search(line):
            out.append({"rule": "todo_leak",
                        "message": "codegen TODO left in Nebula output",
                        "line": i,
                        "excerpt": line.strip()[:120]})
    return out


FN_COLON_RE = re.compile(r"\bfn\s*\(\s*\)\s*:")


def stray_fn_check(text: str) -> list[dict]:
    out = []
    for i, line in enumerate(text.splitlines(), 1):
        if FN_COLON_RE.search(line):
            out.append({"rule": "stray_fn_colon",
                        "message": "'fn():' mid-argument — indent block where expr expected",
                        "line": i,
                        "excerpt": line.strip()[:120]})
    return out


EMPTY_BODY_RE = re.compile(r"\b(?:fn|def)\s+\w[\w]*\s*\([^)]*\)\s*(?:->\s*[\w\[\],\s]*)?\s*:\s*$")


def empty_body_check(text: str) -> list[dict]:
    out = []
    lines = text.splitlines()
    for i, line in enumerate(lines):
        stripped = line.rstrip()
        if EMPTY_BODY_RE.search(stripped):
            cur_indent = len(line) - len(line.lstrip(" "))
            j = i + 1
            has_body = False
            while j < len(lines):
                nxt = lines[j]
                if nxt.strip() == "":
                    j += 1
                    continue
                nxt_indent = len(nxt) - len(nxt.lstrip(" "))
                if nxt_indent > cur_indent:
                    has_body = True
                break
            if not has_body:
                out.append({"rule": "empty_fn_body",
                            "message": "function has ':' opener but no indented body",
                            "line": i + 1,
                            "excerpt": stripped[:120]})
    return out


RAW_TYPE_RE = re.compile(
    r"\b(Float(?:32|64)|Int(?:8|16|32|64)|Uint(?:8|16|32|64)|Byte|Rune|Complex(?:64|128)|Void)\b"
)


def raw_type_check(text: str) -> list[dict]:
    out = []
    for i, raw in enumerate(text.splitlines(), 1):
        cleaned = re.sub(r'"[^"]*"', "", raw)
        cleaned = re.sub(r"'[^']*'", "", cleaned)
        for m in RAW_TYPE_RE.finditer(cleaned):
            out.append({"rule": "raw_type_leak",
                        "message": f"uppercase type '{m.group(1)}' in Nebula output",
                        "line": i,
                        "excerpt": raw.strip()[:120]})
    return out


IDENT_RE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\b")


def casing_duplicate_check(text: str) -> list[dict]:
    clean = _strip_line_comments_and_strings(text)
    seen: dict[str, set[str]] = {}
    for m in IDENT_RE.finditer(clean):
        name = m.group(1)
        if name in TOKEN_KEYWORDS:
            continue
        if len(name) < 4:
            continue
        key = name.lower().replace("_", "")
        seen.setdefault(key, set()).add(name)
    out = []
    for variants in seen.values():
        if len(variants) >= 2:
            has_upper = any(re.search(r"[A-Z]", v) for v in variants)
            has_snake = any("_" in v for v in variants)
            if has_upper and has_snake:
                out.append({"rule": "casing_variants",
                            "message": f"identifier in multiple casings: {sorted(variants)}"})
    return out


def scan_antipatterns(text: str) -> list[dict]:
    out: list[dict] = []
    out.extend(balance_check(text))
    out.extend(todo_check(text))
    out.extend(stray_fn_check(text))
    out.extend(empty_body_check(text))
    out.extend(raw_type_check(text))
    out.extend(casing_duplicate_check(text))
    return out


# ------------------------- phase 2: nebula check -------------------------


CHECK_LOC_RE = re.compile(r"(?:line\s+)?(\d+)\s*[:,]\s*(?:col\s+)?(\d+)", re.IGNORECASE)


def format_check_err(rc: int, stderr: str) -> str:
    msg = stderr.strip()
    if not msg:
        return f"err: exit={rc}"
    m = CHECK_LOC_RE.search(msg)
    if m:
        return f"err at L{m.group(1)}:C{m.group(2)} {trunc(msg, 200)}"
    return f"err: {trunc(msg, 220)}"


# ------------------------- phase 4: native compile ---------------------


def _find(*bins: str) -> Optional[str]:
    for b in bins:
        p = shutil.which(b)
        if p:
            return p
    return None


PY = _find("python3", "python")
NODE = _find("node")
GO = _find("go")
RUSTC = _find("rustc")
SWIFT = _find("swift")
KOTLINC = _find("kotlinc")
CC = _find("cc", "clang", "gcc")


def compile_native(lang: str, code: str) -> str:
    """Return 'ok' or 'err: <msg>' (or 'skip: <reason>')."""
    if not code.strip():
        return "skip: empty roundtrip output"
    with tempfile.TemporaryDirectory() as td:
        tdp = Path(td)
        try:
            if lang == "python":
                if not PY:
                    return "skip: python3 not found"
                # ast.parse catches syntax errors without executing imports
                rc, _, err = run([PY, "-c",
                                  "import ast, sys; ast.parse(sys.stdin.read())"],
                                 stdin_text=code, timeout=15)
                return "ok" if rc == 0 else f"err: {trunc(err, 220)}"

            if lang in ("javascript", "typescript"):
                if not NODE:
                    return "skip: node not found"
                ext = "js" if lang == "javascript" else "ts"
                f = tdp / f"probe.{ext}"
                f.write_text(code)
                # node --check only supports JS; for TS fall back to syntax check via node require parse if possible
                if lang == "javascript":
                    rc, _, err = run([NODE, "--check", str(f)], timeout=15)
                    return "ok" if rc == 0 else f"err: {trunc(err, 220)}"
                # typescript: try tsc if available, else just report skip (the brief says this is fine)
                tsc = shutil.which("tsc")
                if tsc:
                    rc, _, err = run([tsc, "--noEmit", "--target", "es2020",
                                      "--module", "nodenext", "--moduleResolution", "nodenext",
                                      "--skipLibCheck", str(f)], cwd=tdp, timeout=30)
                    if rc == 0:
                        return "ok"
                    return f"err: {trunc(err or 'tsc failed', 220)}"
                return "skip: tsc not installed"

            if lang == "go":
                if not GO:
                    return "skip: go not found"
                # set up a tiny module so `go build` doesn't scream
                f = tdp / "main.go"
                # Force main package so build can complete without imports
                # BUT our probes use `package foo`. We'll use `go vet` on a module.
                (tdp / "go.mod").write_text("module probe\n\ngo 1.21\n")
                # Preserve user package — write as-is
                f.write_text(code)
                env = os.environ.copy()
                env["GO111MODULE"] = "on"
                env["GOFLAGS"] = "-mod=mod"
                env["GOPATH"] = str(tdp / ".gopath")
                env["GOCACHE"] = str(tdp / ".gocache")
                rc, _, err = run([GO, "build", "./..."], cwd=tdp, env=env, timeout=60)
                if rc == 0:
                    return "ok"
                # Fall back to `go vet` which is more tolerant of missing mains
                rc2, _, err2 = run([GO, "vet", "./..."], cwd=tdp, env=env, timeout=60)
                if rc2 == 0:
                    return "ok"
                return f"err: {trunc(err or err2, 240)}"

            if lang == "rust":
                if not RUSTC:
                    return "skip: rustc not found"
                f = tdp / "probe.rs"
                f.write_text(code)
                # --emit=metadata, no-run, allow deadcode
                rc, _, err = run([RUSTC, "--crate-type=lib", "--emit=metadata",
                                  "--edition=2021", "-A", "warnings",
                                  "-o", str(tdp / "out.rmeta"), str(f)],
                                 cwd=tdp, timeout=60)
                return "ok" if rc == 0 else f"err: {trunc(err, 240)}"

            if lang == "swift":
                if not SWIFT:
                    return "skip: swift not found"
                f = tdp / "probe.swift"
                f.write_text(code)
                # parse-only via frontend
                rc, _, err = run([SWIFT, "-frontend", "-parse", str(f)], timeout=30)
                return "ok" if rc == 0 else f"err: {trunc(err, 240)}"

            if lang == "kotlin":
                if not KOTLINC:
                    return "skip: kotlinc not found"
                f = tdp / "probe.kt"
                f.write_text(code)
                # kotlinc needs kotlinx-coroutines-core on the classpath for
                # probes that use the coroutines/flow builders. It ships bundled
                # with the homebrew kotlinc distribution.
                cp_jars = []
                kotlinc_lib = Path(KOTLINC).resolve().parent.parent / "libexec" / "lib"
                if kotlinc_lib.is_dir():
                    for jar in ("kotlinx-coroutines-core-jvm.jar",):
                        p = kotlinc_lib / jar
                        if p.exists():
                            cp_jars.append(str(p))
                cmd = [KOTLINC, "-nowarn", "-d", str(tdp / "out")]
                if cp_jars:
                    cmd += ["-cp", ":".join(cp_jars)]
                cmd.append(str(f))
                rc, _, err = run(cmd, timeout=90)
                return "ok" if rc == 0 else f"err: {trunc(err, 240)}"

            if lang == "c":
                if not CC:
                    return "skip: cc not found"
                f = tdp / "probe.c"
                f.write_text(code)
                # -fsyntax-only — don't link
                rc, _, err = run([CC, "-fsyntax-only", "-w", str(f)], timeout=30)
                return "ok" if rc == 0 else f"err: {trunc(err, 240)}"

            return f"skip: no handler for {lang}"
        except Exception as e:
            return f"err: scanner-exception {e}"


# ------------------------------ driver ------------------------------


def severity_of(row: ProbeRow) -> str:
    # critical: phase 1 or 3 errored, or phase 2 parse-check failed
    if (row.phase1_from_nebula.startswith("err")
            or row.phase2_nebula_check.startswith("err")
            or row.phase3_to_nebula.startswith("err")):
        return "critical"
    # major: native compile failed (and wasn't skipped)
    if row.phase4_native_compile.startswith("err"):
        return "major"
    # minor: antipatterns only
    if row.antipatterns:
        return "minor"
    return "clean"


def run_probe(path: Path, lang: str) -> ProbeRow:
    rel = str(path.relative_to(ROOT))
    idiom = path.stem
    row = ProbeRow(file=rel, lang=lang, idiom=idiom)

    # Phase 1: from-<lang>
    rc1, out1, err1 = run([str(NEBULA_BIN), FROM_SUBCMD[lang]], input_path=path)
    if rc1 != 0:
        row.phase1_from_nebula = f"err: exit={rc1} {trunc(err1, 220)}"
        # no Nebula output to feed downstream phases
        row.phase2_nebula_check = "err: no Nebula output"
        row.phase3_to_nebula = "err: no Nebula output"
        row.phase4_native_compile = "err: no Nebula output"
        row.severity = severity_of(row)
        return row
    row.phase1_from_nebula = "ok"
    row.nebula_output = out1
    row.nebula_len = len(out1)

    # Anti-patterns on the Nebula text
    ap = scan_antipatterns(out1)
    row.antipattern_detail = ap
    row.antipatterns = sorted({f["rule"] for f in ap})

    # Phase 2: nebula check
    with tempfile.NamedTemporaryFile(mode="w", suffix=".neb", delete=False) as tf:
        tf.write(out1)
        tf.flush()
        check_path = Path(tf.name)
    try:
        rc2, _, err2 = run([str(NEBULA_BIN), "check"], input_path=check_path)
    finally:
        check_path.unlink(missing_ok=True)
    row.phase2_nebula_check = "ok" if rc2 == 0 else format_check_err(rc2, err2)

    # Phase 3: to-<lang> (round-trip)
    with tempfile.NamedTemporaryFile(mode="w", suffix=".neb", delete=False) as tf:
        tf.write(out1)
        tf.flush()
        rt_path = Path(tf.name)
    try:
        rc3, out3, err3 = run([str(NEBULA_BIN), TO_SUBCMD[lang]], input_path=rt_path)
    finally:
        rt_path.unlink(missing_ok=True)
    if rc3 != 0:
        row.phase3_to_nebula = f"err: exit={rc3} {trunc(err3, 220)}"
        row.phase4_native_compile = "err: no roundtrip output"
        row.severity = severity_of(row)
        return row
    row.phase3_to_nebula = "ok"
    row.roundtrip_output = out3

    # Phase 4: compile the emitted code natively
    row.phase4_native_compile = compile_native(lang, out3)

    row.severity = severity_of(row)
    return row


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--lang", action="append", help="restrict to one or more languages")
    ap.add_argument("--catalog", default=str(ROOT / "bench" / "scripts" / "probe_catalog.json"))
    ap.add_argument("--no-native", action="store_true",
                    help="skip phase 4 (native compile)")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args()

    if not NEBULA_BIN.exists():
        print(f"ERROR: {NEBULA_BIN} not found. Run `cargo build --release` in compiler/.",
              file=sys.stderr)
        return 2

    langs = sorted(args.lang) if args.lang else sorted(LANG_BY_DIR.keys())
    rows: list[ProbeRow] = []

    for ldir in langs:
        if ldir not in LANG_BY_DIR:
            print(f"WARN: unknown language dir {ldir}", file=sys.stderr)
            continue
        lang, ext = LANG_BY_DIR[ldir]
        sub = PROBE_DIR / ldir
        if not sub.exists():
            continue
        probes = sorted(sub.glob(f"*.{ext}"))
        if not probes:
            continue
        if not args.quiet:
            print(f"\n== {lang} ({len(probes)} probes) ==")
        for p in probes:
            r = run_probe(p, lang)
            if args.no_native and r.phase3_to_nebula == "ok":
                r.phase4_native_compile = "skip: --no-native"
                r.severity = severity_of(r)
            rows.append(r)
            if not args.quiet:
                tag = r.severity.upper()
                print(f"  [{tag:>8s}] {p.name}")
                if r.phase1_from_nebula != "ok":
                    print(f"            phase1 {r.phase1_from_nebula}")
                if r.phase2_nebula_check != "ok":
                    print(f"            phase2 {r.phase2_nebula_check}")
                if r.phase3_to_nebula != "ok":
                    print(f"            phase3 {r.phase3_to_nebula}")
                if r.phase4_native_compile not in ("ok",) and not r.phase4_native_compile.startswith("skip"):
                    print(f"            phase4 {r.phase4_native_compile}")
                if r.antipatterns:
                    print(f"            antipatterns: {', '.join(r.antipatterns)}")

    # Build catalog. Rows are sorted for idempotency.
    rows.sort(key=lambda r: (r.lang, r.file))

    summary = {
        "total": len(rows),
        "clean": sum(1 for r in rows if r.severity == "clean"),
        "minor": sum(1 for r in rows if r.severity == "minor"),
        "major": sum(1 for r in rows if r.severity == "major"),
        "critical": sum(1 for r in rows if r.severity == "critical"),
        "by_lang": {},
    }
    for r in rows:
        bl = summary["by_lang"].setdefault(r.lang, {
            "total": 0, "clean": 0, "minor": 0, "major": 0, "critical": 0,
        })
        bl["total"] += 1
        bl[r.severity] = bl.get(r.severity, 0) + 1

    out_rows = []
    for r in rows:
        out_rows.append({
            "file": r.file,
            "lang": r.lang,
            "idiom": r.idiom,
            "phase1_from_nebula": r.phase1_from_nebula,
            "phase2_nebula_check": r.phase2_nebula_check,
            "phase3_to_nebula": r.phase3_to_nebula,
            "phase4_native_compile": r.phase4_native_compile,
            "antipatterns": r.antipatterns,
            "nebula_len": r.nebula_len,
            "severity": r.severity,
            "antipattern_detail": r.antipattern_detail,
        })

    catalog = {"summary": summary, "probes": out_rows}
    out_path = Path(args.catalog)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(catalog, indent=2, sort_keys=False))
    if not args.quiet:
        print(f"\nCatalog: {out_path}")
        print(f"Summary: {summary}")

    return 1 if (summary["critical"] + summary["major"]) > 0 else 0


if __name__ == "__main__":
    sys.exit(main())
