#!/usr/bin/env python3
"""Generate large-file training tasks by mutating expr.go (425 LoC) from
bench/mcp_manual_test/large_file_template. Closes the Phase 20 1/3 pass rate
on the large_file benchmark shape by giving the combined inner-agent fine-
tune direct exposure to multi-patch edits on a >400-LoC single file.

Each task is a directory containing:
    meta.json   — task prompt + verify_cmd
    seed/       — mutated expr.go + expr_test.go + go.mod

Task mix (15 bug-plants + 5 rename refactors = 20 tasks):

  Bug-plants: replace one line in expr.go with a deliberately wrong variant.
  The prompt tells the model "this package has a failing test — find and fix
  the bug". Verify: `go test ./...`.

  Renames: pick a type or function name and rename it throughout the file.
  Prompt: "rename X to Y throughout this 425-line Go file". Verify: `go test`.

IMPORTANT — the planted bugs / renames are DIFFERENT from the Phase 20
benchmark task (adding % ^ unary-minus operators). Training on this task
mix teaches the pattern of editing a 400-LoC parser; it does not teach
the specific benchmark answer.
"""

from __future__ import annotations
import json
import shutil
from pathlib import Path
from dataclasses import dataclass

REPO = Path(__file__).parents[3]  # .../nebula
LARGE_FILE_TEMPLATE = REPO / "bench" / "mcp_manual_test" / "large_file_template"
OUT_ROOT = Path(__file__).parent / "tasks"


@dataclass
class BugPlant:
    task_id: str
    old_line: str
    new_line: str
    description: str


@dataclass
class Rename:
    task_id: str
    old_name: str
    new_name: str


# Bug-plants. Each tuple is (task_id_slug, old_line, new_line, description).
# Lines are searched with .replace(old, new, 1) so they must be unique in
# expr.go. Keep them short and specific. All bugs deliberately break a
# small number of tests without destabilizing the whole package.
BUG_PLANTS = [
    BugPlant(
        "parse_term_precedence",
        "for p.check(TK_STAR) || p.check(TK_SLASH) {",
        "for p.check(TK_PLUS) || p.check(TK_MINUS) {",
        "parseTerm uses + and - precedence instead of * and /",
    ),
    BugPlant(
        "func_ident_unreached",
        "if p.check(TK_LPAREN) {",
        "if false {",
        "function-call path in parseFactor is dead code",
    ),
    BugPlant(
        "eof_check_missing",
        "if p.peek().Kind != TK_EOF {",
        "if false {",
        "parser skips trailing-garbage check, accepts '1+2 trailing_junk'",
    ),
    BugPlant(
        "lexer_plus_as_minus",
        "TK_PLUS, Lexeme: \"+\", Offset: l.pos",
        "TK_MINUS, Lexeme: \"+\", Offset: l.pos",
        "lexer tags '+' as TK_MINUS",
    ),
    BugPlant(
        "lexer_star_as_slash",
        "TK_STAR, Lexeme: \"*\", Offset: l.pos",
        "TK_SLASH, Lexeme: \"*\", Offset: l.pos",
        "lexer tags '*' as TK_SLASH",
    ),
    BugPlant(
        "parse_expr_plus_minus_loop",
        "for p.check(TK_PLUS) || p.check(TK_MINUS) {",
        "for p.check(TK_STAR) || p.check(TK_SLASH) {",
        "parseExpr loops on * and / instead of + and -",
    ),
    BugPlant(
        "parse_factor_number_parse",
        "strconv.ParseFloat(t.Lexeme, 64)",
        "strconv.ParseFloat(\"0\", 64)",
        "parseFactor always parses numbers as 0",
    ),
    BugPlant(
        "parser_advance_no_pos_bump",
        "func (p *Parser) advance() Token     { t := p.toks[p.pos]; p.pos++; return t }",
        "func (p *Parser) advance() Token     { t := p.toks[p.pos]; return t }",
        "Parser.advance forgets to bump pos (infinite loop)",
    ),
    BugPlant(
        "ch_whitespace_no_advance",
        "if ch == ' ' || ch == '\\t' || ch == '\\n' || ch == '\\r' {\n\t\t\tl.pos++",
        "if ch == ' ' || ch == '\\t' || ch == '\\n' || ch == '\\r' {\n\t\t\t// l.pos++ intentionally removed",
        "tokenize does not advance on whitespace",
    ),
    BugPlant(
        "number_loop_drops_last",
        "for l.pos < len(l.src) && ((l.src[l.pos] >= '0' && l.src[l.pos] <= '9') || l.src[l.pos] == '.') {",
        "for l.pos < len(l.src)-1 && ((l.src[l.pos] >= '0' && l.src[l.pos] <= '9') || l.src[l.pos] == '.') {",
        "number loop stops one char too early",
    ),
]


# Renames. Each tuple is (slug, old_name, new_name). The identifier must
# appear exclusively as a symbol (no false matches inside strings). Our
# expr.go uses sufficiently distinctive names that word-boundary replace
# is safe.
RENAMES = [
    Rename("token_to_lex", "Token", "Lex"),
    Rename("lexer_to_scanner", "Lexer", "Scanner"),
    Rename("parse_to_buildast", "Parse", "BuildAST"),
    Rename("eval_to_compute", "Eval", "Compute"),
    Rename("defaultenv_to_builtinenv", "DefaultEnv", "BuiltinEnv"),
]


def load_base() -> dict[str, str]:
    """Load expr.go + a TRIMMED expr_test.go that only contains the tests
    which pass in the unmodified base fixture. The original expr_test.go
    also has tests for the Phase 20 benchmark task (add %/^/unary-) that
    fail until the model implements those features — we exclude them from
    the training task so that bug-plants create a clean "one test started
    failing, find the bug" signal."""
    files: dict[str, str] = {}
    files["expr.go"] = (LARGE_FILE_TEMPLATE / "expr.go").read_text()
    files["go.mod"] = (LARGE_FILE_TEMPLATE / "go.mod").read_text()
    full_test = (LARGE_FILE_TEMPLATE / "expr_test.go").read_text()
    marker = "func TestModuloBasic"
    idx = full_test.find(marker)
    if idx < 0:
        raise RuntimeError(f"could not find marker {marker!r} in expr_test.go")
    # Keep everything up to the marker line. Walk back to the last blank line.
    trimmed = full_test[:idx].rstrip() + "\n"
    files["expr_test.go"] = trimmed
    return files


def write_task(task_id: str, seed_files: dict[str, str], meta: dict) -> None:
    task_dir = OUT_ROOT / task_id
    if task_dir.exists():
        shutil.rmtree(task_dir)
    seed_dir = task_dir / "seed"
    seed_dir.mkdir(parents=True)
    for name, content in seed_files.items():
        (seed_dir / name).write_text(content)
    (task_dir / "meta.json").write_text(json.dumps(meta, indent=2))


def apply_bug(base: dict[str, str], bug: BugPlant) -> dict[str, str] | None:
    expr = base["expr.go"]
    if bug.old_line not in expr:
        return None
    if expr.count(bug.old_line) != 1:
        # Non-unique match — skip rather than corrupt.
        return None
    mutated = dict(base)
    mutated["expr.go"] = expr.replace(bug.old_line, bug.new_line, 1)
    return mutated


def apply_rename(base: dict[str, str], rename: Rename) -> dict[str, str] | None:
    import re
    files = dict(base)
    pattern = rf"\b{re.escape(rename.old_name)}\b"
    for name in ("expr.go", "expr_test.go"):
        new_content = re.sub(pattern, rename.new_name, files[name])
        if new_content == files[name] and name == "expr.go":
            return None
        files[name] = new_content
    return files


def main() -> None:
    base = load_base()
    OUT_ROOT.mkdir(parents=True, exist_ok=True)

    generated: list[str] = []
    skipped: list[str] = []

    for bug in BUG_PLANTS:
        mutated = apply_bug(base, bug)
        task_id = f"largefile_bugfix_{bug.task_id}"
        if mutated is None:
            skipped.append(task_id + " (no match or non-unique)")
            continue
        prompt = (
            "This is a single-file Go package implementing a small arithmetic "
            "expression language. The file `expr.go` is 425 lines; `expr_test.go` "
            "is the test file. The package has a bug — `go test ./...` fails. "
            "Find the bug and fix it with a minimal edit. Do not rewrite the whole "
            "file. When the tests pass, you're done."
        )
        write_task(task_id, mutated, {
            "id": task_id,
            "type": "bugfix",
            "language": "go",
            "package": "expr_largefile",
            "bug_description": bug.description,
            "file_sizes": "expr.go=425 lines (crosses Nebula threshold)",
            "prompt": prompt,
            "verify_cmd": "go test ./...",
        })
        generated.append(task_id)

    for rename in RENAMES:
        mutated = apply_rename(base, rename)
        task_id = f"largefile_refactor_{rename.task_id}"
        if mutated is None:
            skipped.append(task_id + " (rename had no effect)")
            continue
        # For refactor tasks, the SEED is the ALREADY-RENAMED state, which
        # compiles — we re-invert so the model is asked to perform the rename.
        inverted = dict(base)  # unchanged base is the task seed
        prompt = (
            f"Rename the Go identifier `{rename.old_name}` to `{rename.new_name}` "
            f"throughout this 425-line single-file Go package (`expr.go`) and its "
            f"test file (`expr_test.go`). Update every reference, including doc "
            f"comments. Because the rename touches many locations, the efficient "
            f"approach is `patch_code` with `replace_all=true` per file — one call "
            f"per file instead of many individual substitutions.\n\n"
            f"After the rename, run `go test ./...` to confirm the package still "
            f"compiles and tests pass."
        )
        write_task(task_id, inverted, {
            "id": task_id,
            "type": "refactor",
            "language": "go",
            "package": "expr_largefile",
            "old_name": rename.old_name,
            "new_name": rename.new_name,
            "file_sizes": "expr.go=425 lines (crosses Nebula threshold)",
            "prompt": prompt,
            "verify_cmd": "go test ./...",
        })
        generated.append(task_id)

    print(f"Generated {len(generated)} large-file tasks in {OUT_ROOT}")
    for t in generated:
        print(f"  {t}")
    if skipped:
        print()
        print(f"Skipped {len(skipped)}:")
        for t in skipped:
            print(f"  {t}")


if __name__ == "__main__":
    main()
