#!/usr/bin/env python3
"""Generate large-file training tasks from 3 fixtures:
  1. expr.go (425 LoC, arithmetic expression parser)
  2. kvproj.go (423 LoC, consolidated KV store)
  3. csvparser.go (309 LoC, CSV parser)

Each fixture gets 15-20 task variants (bugfix + refactor), for a total of
~50 large-file training tasks. This is the v2 expansion that targets the
Phase 20 large_file 1/3 pass rate by giving the model diverse exposure to
multi-patch edits on 300-425 LoC single-file Go packages.
"""

from __future__ import annotations
import json
import re
import shutil
from pathlib import Path
from dataclasses import dataclass

REPO = Path(__file__).parents[3]
FIXTURES_DIR = Path(__file__).parent / "fixtures"
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


@dataclass
class Fixture:
    name: str
    go_file: str
    test_file: str
    go_mod: str
    bugs: list
    renames: list
    trim_test_marker: str | None = None  # if set, trim test file at this marker


def load_fixture(f: Fixture) -> dict[str, str]:
    files = {"go.mod": f.go_mod}
    go_src = Path(f.go_file).read_text()
    test_src = Path(f.test_file).read_text()
    if f.trim_test_marker:
        idx = test_src.find(f.trim_test_marker)
        if idx >= 0:
            test_src = test_src[:idx].rstrip() + "\n"
    # Determine filenames from the path
    go_name = Path(f.go_file).name
    test_name = Path(f.test_file).name
    files[go_name] = go_src
    files[test_name] = test_src
    return files


def apply_bug(files: dict[str, str], bug: BugPlant, go_name: str) -> dict[str, str] | None:
    src = files[go_name]
    if bug.old_line not in src or src.count(bug.old_line) != 1:
        return None
    out = dict(files)
    out[go_name] = src.replace(bug.old_line, bug.new_line, 1)
    return out


def apply_rename(files: dict[str, str], rename: Rename) -> dict[str, str] | None:
    pattern = rf"\b{re.escape(rename.old_name)}\b"
    out = dict(files)
    any_change = False
    for name in out:
        if name.endswith(".go"):
            new_content = re.sub(pattern, rename.new_name, out[name])
            if new_content != out[name]:
                any_change = True
            out[name] = new_content
    return out if any_change else None


def write_task(task_id: str, seed_files: dict[str, str], meta: dict) -> None:
    task_dir = OUT_ROOT / task_id
    if task_dir.exists():
        shutil.rmtree(task_dir)
    seed_dir = task_dir / "seed"
    seed_dir.mkdir(parents=True)
    for name, content in seed_files.items():
        (seed_dir / name).write_text(content)
    (task_dir / "meta.json").write_text(json.dumps(meta, indent=2))


GOMOD = "module nebulatest\n\ngo 1.21\n"

# ── Fixture definitions ──────────────────────────────────────────────────

EXPR_FIXTURE = Fixture(
    name="expr",
    go_file=str(LARGE_FILE_TEMPLATE / "expr.go"),
    test_file=str(LARGE_FILE_TEMPLATE / "expr_test.go"),
    go_mod=GOMOD,
    trim_test_marker="func TestModuloBasic",
    bugs=[
        BugPlant("parse_term_prec", "for p.check(TK_STAR) || p.check(TK_SLASH) {", "for p.check(TK_PLUS) || p.check(TK_MINUS) {", "parseTerm uses wrong precedence"),
        BugPlant("func_unreached", "if p.check(TK_LPAREN) {", "if false {", "function-call path dead code"),
        BugPlant("eof_skip", "if p.peek().Kind != TK_EOF {", "if false {", "trailing-garbage check skipped"),
        BugPlant("plus_as_minus", 'TK_PLUS, Lexeme: "+", Offset: l.pos', 'TK_MINUS, Lexeme: "+", Offset: l.pos', "lexer tags + as -"),
        BugPlant("star_as_slash", 'TK_STAR, Lexeme: "*", Offset: l.pos', 'TK_SLASH, Lexeme: "*", Offset: l.pos', "lexer tags * as /"),
        BugPlant("expr_wrong_loop", "for p.check(TK_PLUS) || p.check(TK_MINUS) {", "for p.check(TK_STAR) || p.check(TK_SLASH) {", "parseExpr loops on wrong ops"),
        BugPlant("num_always_zero", 'strconv.ParseFloat(t.Lexeme, 64)', 'strconv.ParseFloat("0", 64)', "numbers always parse as 0"),
        BugPlant("advance_no_bump", "func (p *Parser) advance() Token     { t := p.toks[p.pos]; p.pos++; return t }", "func (p *Parser) advance() Token     { t := p.toks[p.pos]; return t }", "advance forgets to bump pos"),
        BugPlant("num_drops_last", "for l.pos < len(l.src) && ((l.src[l.pos] >= '0' && l.src[l.pos] <= '9') || l.src[l.pos] == '.') {", "for l.pos < len(l.src)-1 && ((l.src[l.pos] >= '0' && l.src[l.pos] <= '9') || l.src[l.pos] == '.') {", "number loop drops last digit"),
        BugPlant("eval_plus_mul", "case TK_PLUS:\n\t\t\treturn lv + rv, nil", "case TK_PLUS:\n\t\t\treturn lv * rv, nil", "eval + does multiplication"),
    ],
    renames=[
        Rename("token_to_lex", "Token", "Lex"),
        Rename("lexer_to_scanner", "Lexer", "Scanner"),
        Rename("parse_to_build", "Parse", "BuildAST"),
        Rename("eval_to_compute", "Eval", "Compute"),
        Rename("defenv_to_builtin", "DefaultEnv", "BuiltinEnv"),
    ],
)

KVPROJ_FIXTURE = Fixture(
    name="kvproj",
    go_file=str(FIXTURES_DIR / "kvproj.go"),
    test_file=str(FIXTURES_DIR / "kvproj_test.go"),
    go_mod=GOMOD,
    bugs=[
        BugPlant("set_no_store", "c.data[key] = kvEntry{value: value,", "// c.data[key] = kvEntry{value: value,", "Set doesn't actually store"),
        BugPlant("get_wrong_key", 'e, ok := c.data[key]', 'e, ok := c.data["__missing__"]', "Get looks up wrong key"),
        BugPlant("delete_skips", "delete(c.data, key)", "// delete(c.data, key)", "Delete is a no-op"),
        BugPlant("len_always_zero", "return len(c.data)", "return 0", "Len always returns 0"),
        BugPlant("evict_wrong_order", "oldest := c.order[0]", "oldest := c.order[len(c.order)-1]", "evictOldest removes newest instead"),
        BugPlant("ttl_no_expiry", "expiresAt: time.Now().Add(ttl)", "expiresAt: time.Time{}", "SetTTL sets zero expiry"),
    ],
    renames=[
        Rename("kvcache_to_store", "KVCache", "Store"),
        Rename("newkv_to_open", "NewKVCache", "OpenStore"),
        Rename("logger_to_log", "Logger", "Log"),
        Rename("validator_to_check", "Validator", "Checker"),
        Rename("metrics_to_stats", "Metrics", "Stats"),
        Rename("admin_to_mgmt", "AdminTool", "MgmtTool"),
    ],
)

CSV_FIXTURE = Fixture(
    name="csv",
    go_file=str(FIXTURES_DIR / "csvparser.go"),
    test_file=str(FIXTURES_DIR / "csvparser_test.go"),
    go_mod=GOMOD,
    bugs=[
        BugPlant("skip_blank_invert", "if len(strings.TrimSpace(raw)) == 0 {\n\t\t\tcontinue", "if len(strings.TrimSpace(raw)) != 0 {\n\t\t\tcontinue", "blank-line skip is inverted"),
        BugPlant("quote_escape_off", "if i+1 < len(line) && line[i+1] == q {", "if false {", "escaped-quote handling disabled"),
        BugPlant("trim_always", "if r.dialect.TrimSpaces {", "if true {", "TrimSpaces always on"),
        BugPlant("format_no_escape", 'escaped := strings.ReplaceAll(field, q, q+q)', 'escaped := field', "FormatRecord doesn't escape quotes"),
        BugPlant("count_returns_neg", "return len(rec)", "return -1", "CountFields always returns -1"),
        BugPlant("header_numeric_inv", "if allDigits {\n\t\t\treturn false", "if allDigits {\n\t\t\treturn true", "HasHeader inverts numeric check"),
        BugPlant("filter_keeps_all", "if keep(rec) {", "if true {", "FilterRecords keeps everything"),
        BugPlant("map_wrong_col", "if col >= 0 && col < len(row) {", "if col >= 0 && col < len(row)-1 {", "MapField skips last column"),
    ],
    renames=[
        Rename("reader_to_parser", "Reader", "CSVParser"),
        Rename("dialect_to_config", "Dialect", "CSVConfig"),
        Rename("record_to_row", "Record", "Row"),
        Rename("parseerr_to_csverr", "ParseError", "CSVError"),
        Rename("format_to_encode", "FormatRecord", "EncodeRecord"),
    ],
)


def generate_fixture_tasks(fixture: Fixture) -> list[str]:
    base = load_fixture(fixture)
    go_name = Path(fixture.go_file).name
    generated = []
    skipped = []

    for bug in fixture.bugs:
        task_id = f"largefile_bugfix_{fixture.name}_{bug.task_id}"
        mutated = apply_bug(base, bug, go_name)
        if mutated is None:
            skipped.append(task_id)
            continue
        prompt = (
            f"This is a single-file Go package ({go_name}, {len(base[go_name].splitlines())} lines). "
            f"The package has a bug — `go test ./...` fails. Find the bug and fix it with a minimal "
            f"edit. Do not rewrite the whole file. When the tests pass, you're done."
        )
        write_task(task_id, mutated, {
            "id": task_id, "type": "bugfix", "language": "go",
            "package": f"{fixture.name}_largefile",
            "bug_description": bug.description,
            "file_sizes": f"{go_name}={len(base[go_name].splitlines())} lines",
            "prompt": prompt, "verify_cmd": "go test ./...",
        })
        generated.append(task_id)

    for rename in fixture.renames:
        task_id = f"largefile_refactor_{fixture.name}_{rename.task_id}"
        prompt = (
            f"Rename the Go identifier `{rename.old_name}` to `{rename.new_name}` throughout this "
            f"{len(base[go_name].splitlines())}-line single-file Go package (`{go_name}`) and its "
            f"test file. Update every reference, including doc comments. Use `patch_code` with "
            f"`replace_all=true` per file for efficiency.\n\n"
            f"After the rename, run `go test ./...` to confirm the package still compiles and tests pass."
        )
        write_task(task_id, dict(base), {
            "id": task_id, "type": "refactor", "language": "go",
            "package": f"{fixture.name}_largefile",
            "old_name": rename.old_name, "new_name": rename.new_name,
            "file_sizes": f"{go_name}={len(base[go_name].splitlines())} lines",
            "prompt": prompt, "verify_cmd": "go test ./...",
        })
        generated.append(task_id)

    if skipped:
        print(f"  Skipped {len(skipped)} ({', '.join(skipped[:5])}{'...' if len(skipped) > 5 else ''})")
    return generated


def main():
    OUT_ROOT.mkdir(parents=True, exist_ok=True)
    all_tasks = []
    for fixture in [EXPR_FIXTURE, KVPROJ_FIXTURE, CSV_FIXTURE]:
        print(f"\n=== {fixture.name} ({Path(fixture.go_file).name}) ===")
        tasks = generate_fixture_tasks(fixture)
        print(f"  Generated {len(tasks)} tasks")
        all_tasks.extend(tasks)

    # Update the task index
    index_path = OUT_ROOT / "task_index.json"
    if index_path.exists():
        idx = json.loads(index_path.read_text())
        existing = set(idx.get("tasks", []))
    else:
        existing = set()
    existing.update(all_tasks)
    counts = {}
    for t in existing:
        meta_path = OUT_ROOT / t / "meta.json"
        if meta_path.exists():
            meta = json.loads(meta_path.read_text())
            typ = meta.get("type", "unknown")
            counts[typ] = counts.get(typ, 0) + 1
    index_path.write_text(json.dumps({
        "total": len(existing), "counts": counts, "tasks": sorted(existing),
    }, indent=2))

    print(f"\nTotal new large-file tasks: {len(all_tasks)}")
    print(f"Updated index: {len(existing)} total tasks")


if __name__ == "__main__":
    main()
