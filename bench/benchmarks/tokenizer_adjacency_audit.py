#!/usr/bin/env python3
"""cl100k_base adjacency audit for Nebula 0.3 grammar design.

Measures how tiktoken's cl100k_base tokenizes every syntax-relevant character,
keyword, and adjacency pattern we might use in a Nebula redesign. Writes a
markdown report with the decoded pieces for every pattern, flagging which
patterns are 1 token (free) vs multi-token (expensive).

The design questions this answers:
  1. Which Nebula-relevant characters and pairs are single-token?
  2. Which adjacency patterns unlock BPE merges? (e.g., `){` vs `) {`)
  3. How does `\\n    ` (newline + indent) tokenize at different depths?
  4. How do Python keywords compare to Nebula's compressed variants?
  5. What are the hidden 1-token wins for C-family syntax?
"""

from __future__ import annotations
import tiktoken
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
OUT = REPO_ROOT / "bench" / "results" / "tokenizer_adjacency_audit.md"

ENC = tiktoken.get_encoding("cl100k_base")


def tok(text: str) -> list[str]:
    ids = ENC.encode(text)
    return [ENC.decode([i]) for i in ids]


def count(text: str) -> int:
    return len(ENC.encode(text))


def row(label: str, text: str) -> tuple[str, str, int, str]:
    pieces = tok(text)
    # Repr for display (show \n, \t, spaces as literals).
    display = text.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t")
    pieces_display = " | ".join(
        p.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t").replace("|", "\\|")
        for p in pieces
    )
    return (label, display, len(pieces), pieces_display)


def emit_table(title: str, rows: list[tuple[str, str, int, str]]) -> str:
    out = [f"## {title}", ""]
    out.append("| Label | Text | Tokens | Pieces |")
    out.append("|---|---|---|---|")
    for label, display, n, pieces in rows:
        # Escape pipe inside text column.
        text_esc = display.replace("|", "\\|")
        out.append(f"| {label} | `{text_esc}` | **{n}** | `{pieces}` |")
    out.append("")
    return "\n".join(out)


def main() -> None:
    sections: list[str] = []

    # 1. Single characters and core operators
    chars = [
        ("open brace", "{"),
        ("close brace", "}"),
        ("open paren", "("),
        ("close paren", ")"),
        ("open bracket", "["),
        ("close bracket", "]"),
        ("colon", ":"),
        ("semicolon", ";"),
        ("comma", ","),
        ("pipe", "|"),
        ("arrow", "->"),
        ("fat arrow", "=>"),
        ("colon colon", "::"),
        ("pipe arrow", "|>"),
        ("nil coalesce", "??"),
        ("dot dot", ".."),
        ("dot dot eq", "..="),
        ("eq", "="),
        ("eq eq", "=="),
        ("walrus", ":="),
        ("plus eq", "+="),
        ("minus eq", "-="),
    ]
    sections.append(emit_table(
        "1. Single chars and operators (no surrounding context)",
        [row(l, t) for l, t in chars],
    ))

    # 2. Brace-adjacency: packed vs spaced
    adjacency = [
        ("paren-brace packed", "){"),
        ("paren-brace spaced", ") {"),
        ("close brace newline", "}\n"),
        ("close brace space", "} "),
        ("newline close brace", "\n}"),
        ("close then else (packed)", "}else{"),
        ("close then else (spaced)", "} else {"),
        ("close then else:", "} else:"),
        ("paren colon (Python)", "):"),
        ("paren colon newline indent", "):\n    "),
        ("close brace newline indent", "}\n    "),
        ("paren brace newline", "){\n"),
        ("paren brace newline indent", "){\n    "),
        ("close brace newline close brace", "}\n}"),
    ]
    sections.append(emit_table(
        "2. Brace adjacency — packed vs spaced",
        [row(l, t) for l, t in adjacency],
    ))

    # 3. Newline + indent patterns
    newlines = [
        ("bare newline", "\n"),
        ("newline + 2 spaces", "\n  "),
        ("newline + 4 spaces", "\n    "),
        ("newline + 8 spaces", "\n        "),
        ("newline + 12 spaces", "\n            "),
        ("newline + 16 spaces", "\n                "),
        ("double newline + 4 spaces", "\n\n    "),
        ("newline + tab", "\n\t"),
        ("newline + 2 tabs", "\n\t\t"),
    ]
    sections.append(emit_table(
        "3. Indentation costs (newline + leading whitespace)",
        [row(l, t) for l, t in newlines],
    ))

    # 4. Block openers: Python vs Nebula
    openers = [
        ("Python def block", "def f():\n    "),
        ("Python if block", "if x:\n    "),
        ("Python for block", "for x in y:\n    "),
        ("Python while block", "while x:\n    "),
        ("Nebula fn brace", "fn f() {\n    "),
        ("Nebula fn packed", "fn f(){\n    "),
        ("Nebula if brace", "if x {\n    "),
        ("Nebula if packed", "if x{\n    "),
        ("Nebula fo brace", "fo x in y {\n    "),
        ("Nebula fo packed", "fo x in y{\n    "),
        ("Nebula wl brace", "wl x {\n    "),
        ("Nebula match brace", "mc x {\n    "),
        ("hypothetical fn colon", "fn f():\n    "),
        ("hypothetical fo colon", "fo x in y:\n    "),
    ]
    sections.append(emit_table(
        "4. Full block openers — Python vs Nebula vs hypothetical",
        [row(l, t) for l, t in openers],
    ))

    # 5. Keywords standalone
    keywords = [
        ("def", " def"),
        ("fn", " fn"),
        ("return", " return"),
        ("rt", " rt"),
        ("let", " let"),
        ("lt", " lt"),
        ("mt", " mt"),
        ("const", " const"),
        ("if", " if"),
        ("else", " else"),
        ("el", " el"),
        ("elif", " elif"),
        ("for", " for"),
        ("fo", " fo"),
        ("while", " while"),
        ("wl", " wl"),
        ("loop", " loop"),
        ("lp", " lp"),
        ("match", " match"),
        ("mc", " mc"),
        ("class", " class"),
        ("st", " st"),
        ("enum", " enum"),
        ("en", " en"),
        ("impl", " impl"),
        ("im", " im"),
        ("trait", " trait"),
        ("tr", " tr"),
        ("True", " True"),
        ("tt", " tt"),
        ("False", " False"),
        ("ff", " ff"),
        ("None", " None"),
        ("nil", " nil"),
        ("self", " self"),
        ("sf", " sf"),
        ("lambda", " lambda"),
        ("import", " import"),
        ("from", " from"),
        ("as", " as"),
        ("in", " in"),
        ("pass", " pass"),
        ("break", " break"),
        ("bk", " bk"),
        ("continue", " continue"),
        ("ct", " ct"),
        ("try", " try"),
        ("except", " except"),
        ("raise", " raise"),
        ("with", " with"),
        ("yield", " yield"),
    ]
    sections.append(emit_table(
        "5. Keyword comparison (leading space, no trailing punctuation)",
        [row(l, t) for l, t in keywords],
    ))

    # 6. Assignment forms
    assignments = [
        ("bare Python assign", "x = 0"),
        ("Nebula lt assign", "lt x = 0"),
        ("Nebula mt assign", "mt x = 0"),
        ("Python with type", "x: int = 0"),
        ("Nebula with type", "lt x: Int = 0"),
        ("aug add", "x += 1"),
        ("aug sub", "x -= 1"),
        ("tuple swap Py", "a, b = b, a"),
        ("walrus", "(x := 0)"),
    ]
    sections.append(emit_table(
        "6. Assignment forms",
        [row(l, t) for l, t in assignments],
    ))

    # 7. Type annotations
    types = [
        ("colon int", ": int"),
        ("colon str", ": str"),
        ("colon bool", ": bool"),
        ("colon List int", ": List[int]"),
        ("colon list int (lower)", ": list[int]"),
        ("colon Tuple", ": Tuple[int, int]"),
        ("colon Optional", ": Optional[int]"),
        ("colon Dict", ": Dict[str, int]"),
        ("arrow int", " -> int"),
        ("arrow List int", " -> List[int]"),
        ("arrow Optional", " -> Optional[int]"),
        ("Nebula Int", ": Int"),
        ("Nebula Float", ": Float"),
        ("Nebula List int", ": List[Int]"),
    ]
    sections.append(emit_table(
        "7. Type annotations",
        [row(l, t) for l, t in types],
    ))

    # 8. Method chain / call patterns
    methods = [
        ("dot append paren", ".append("),
        ("dot join paren", ".join("),
        ("dot split paren", ".split("),
        ("dot map paren", ".map("),
        ("dot filter paren", ".filter("),
        ("dot strip paren", ".strip("),
        ("dot upper paren", ".upper("),
        ("len paren", "len("),
        ("range paren", "range("),
        ("enumerate paren", "enumerate("),
        ("zip paren", "zip("),
        ("sorted paren", "sorted("),
        ("print paren", "print("),
        ("Nebula prt paren", "prt("),
        ("Nebula ln paren", "ln("),
        ("Nebula rng paren", "rng("),
        ("Nebula enu paren", "enu("),
        ("Nebula srt paren", "srt("),
        ("Nebula mp paren", ".mp("),
        ("Nebula fl paren", ".fl("),
        ("Nebula jn paren", ".jn("),
        ("Nebula psh paren", ".psh("),
    ]
    sections.append(emit_table(
        "8. Method call and builtin patterns",
        [row(l, t) for l, t in methods],
    ))

    # 9. Complete statement comparisons — the most important signal
    statements = [
        ("Py simple def", "def add(a, b):\n    return a + b\n"),
        ("Neb braces", "fn add(a, b) {\n    rt a + b\n}\n"),
        ("Neb braces packed", "fn add(a,b){\n    rt a+b\n}\n"),
        ("Neb short-form", "fn add(a, b) = a + b\n"),
        ("Neb indent (proposed)", "fn add(a, b):\n    rt a + b\n"),
        ("Py if", "if x > 0:\n    return x\n"),
        ("Neb if braces", "if x > 0 {\n    rt x\n}\n"),
        ("Neb if indent", "if x > 0:\n    rt x\n"),
        ("Py for append", "for i in range(n):\n    out.append(i)\n"),
        ("Neb for braces", "fo i in rng(n) {\n    out.psh(i)\n}\n"),
        ("Neb for indent", "fo i in rng(n):\n    out.psh(i)\n"),
        ("Py nested", "def f(n):\n    for i in range(n):\n        if i % 2 == 0:\n            print(i)\n"),
        ("Neb braces nested", "fn f(n) {\n    fo i in rng(n) {\n        if i % 2 == 0 {\n            prt(i)\n        }\n    }\n}\n"),
        ("Neb indent nested", "fn f(n):\n    fo i in rng(n):\n        if i % 2 == 0:\n            prt(i)\n"),
    ]
    sections.append(emit_table(
        "9. Complete statement comparisons (the real test)",
        [row(l, t) for l, t in statements],
    ))

    # Header + summary
    header = [
        "# cl100k_base Adjacency Audit for Nebula 0.3",
        "",
        "Measures how `tiktoken.get_encoding('cl100k_base')` tokenizes every",
        "syntax-relevant pattern we might use in a Nebula redesign. Run via",
        "`bench/benchmarks/tokenizer_adjacency_audit.py`.",
        "",
        "Numbers in **bold** are token counts. The `Pieces` column shows how the",
        "tokenizer split the text — watch for single-token wins and for",
        "adjacency merges that could inform grammar choices.",
        "",
    ]

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(header) + "\n".join(sections) + "\n")
    print(f"Wrote {OUT.relative_to(REPO_ROOT)}")

    # Also print a terse console summary of the most important comparisons.
    print()
    print("=" * 60)
    print("HEADLINE COMPARISONS (full statement, fewer = better)")
    print("=" * 60)
    for label, text in statements:
        print(f"{count(text):4d}  {label}")


if __name__ == "__main__":
    main()
