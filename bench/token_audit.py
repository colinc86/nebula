#!/usr/bin/env python3
"""
Nebula Tokenizer Audit

Checks every Nebula keyword and builtin against cl100k_base to find
token-inefficient choices. Identifies keywords where Nebula costs MORE
tokens than the source language equivalent.

Usage: python3 token_audit.py
"""

import tiktoken

enc = tiktoken.get_encoding("cl100k_base")

def tok(s: str) -> int:
    """Count cl100k_base tokens for a string."""
    return len(enc.encode(s))

def tok_detail(s: str) -> list[str]:
    """Show individual tokens for a string."""
    return [enc.decode([t]) for t in enc.encode(s)]

# ── Nebula keywords vs source language equivalents ──

KEYWORDS = [
    # (nebula, source_equivalents, description)
    ("lt",     ["let", "const", "val", "final"],        "immutable binding"),
    ("mt",     ["var", "let mut"],                       "mutable binding"),
    ("cn",     ["const"],                                "constant"),
    ("fn",     ["function", "func", "fun", "def"],       "function"),
    ("rt",     ["return"],                               "return"),
    ("st",     ["struct", "class", "data class"],        "struct/class"),
    ("en",     ["enum", "enum class"],                   "enum"),
    ("tr",     ["trait", "interface", "protocol"],        "trait"),
    ("im",     ["impl"],                                 "impl"),
    ("fo",     ["for"],                                  "for loop"),
    ("wl",     ["while"],                                "while loop"),
    ("lp",     ["loop"],                                 "infinite loop"),
    ("mc",     ["match", "switch", "when"],              "match/switch"),
    ("if",     ["if"],                                   "if"),
    ("el",     ["else"],                                 "else"),
    ("bk",     ["break"],                                "break"),
    ("ct",     ["continue"],                             "continue"),
    ("sf",     ["self", "this"],                         "self reference"),
    ("tt",     ["true", "True"],                         "boolean true"),
    ("ff",     ["false", "False"],                       "boolean false"),
    ("nil",    ["null", "None", "nil", "undefined"],     "null/none"),
    ("pb",     ["pub", "public"],                        "public"),
    ("ac",     ["async"],                                "async"),
    ("aw",     ["await"],                                "await"),
    ("sp",     ["spawn", "go"],                          "spawn/goroutine"),
    ("ty",     ["type", "typedef", "typealias"],         "type alias"),
    ("md",     ["mod", "module", "package"],             "module"),
    ("use",    ["import", "use", "require"],             "import"),
    ("and",    ["&&", "and"],                            "logical and"),
    ("or",     ["||", "or"],                             "logical or"),
    ("not",    ["!", "not"],                             "logical not"),
    ("in",     ["in"],                                   "membership"),
]

BUILTINS = [
    # (nebula_alias, source_equivalents, description)
    ("prt",    ["print", "println", "console.log", "fmt.Println", "printf"],  "print"),
    ("ln",     ["len", "length", "count", "size"],                            "length"),
    ("tsr",    ["str", "to_string", "toString", "String"],                    "to string"),
    ("toi",    ["int", "to_int", "parseInt", "atoi"],                         "to int"),
    ("tof",    ["float", "to_float", "parseFloat", "atof"],                   "to float"),
    ("typ",    ["type", "typeof", "type_of"],                                 "type check"),
    ("rng",    ["range"],                                                      "range"),
    ("enu",    ["enumerate"],                                                  "enumerate"),
    ("psh",    ["push", "append", "push_back"],                               "push/append"),
    ("mp",     ["map"],                                                        "map"),
    ("fl",     ["filter"],                                                     "filter"),
    ("fld",    ["fold", "reduce"],                                             "fold/reduce"),
    ("srt",    ["sort", "sorted"],                                             "sort"),
    ("rev",    ["reverse", "reversed"],                                        "reverse"),
    ("jn",     ["join"],                                                       "join"),
    ("spl",    ["split"],                                                      "split"),
    ("trm",    ["trim", "strip"],                                              "trim"),
    ("ck",     ["contains", "includes", "has"],                                "contains"),
    ("ea",     ["forEach", "for_each", "each"],                                "each"),
    ("fnd",    ["find", "first"],                                              "find"),
    ("abs",    ["abs"],                                                        "absolute"),
    ("min",    ["min"],                                                        "minimum"),
    ("max",    ["max"],                                                        "maximum"),
    ("fmt",    ["format", "sprintf", "fmt.Sprintf"],                           "format string"),
]

METHOD_ALIASES = [
    ("it",     ["iter", "iterator"],                     "iterator"),
    ("col",    ["collect"],                              "collect"),
    ("uw",     ["unwrap"],                               "unwrap"),
    ("emp",    ["isEmpty", "is_empty"],                  "is empty"),
    ("clr",    ["clear", "removeAll"],                   "clear"),
    ("pop",    ["pop", "removeLast"],                    "pop"),
    ("ins",    ["insert"],                               "insert"),
    ("sum",    ["sum"],                                  "sum"),
    ("keys",   ["keys"],                                 "keys"),
    ("vals",   ["values"],                               "values"),
]

def audit_group(name, items):
    print(f"\n{'=' * 70}")
    print(f" {name}")
    print(f"{'=' * 70}")
    print(f"{'Nebula':<8} {'Tok':<4} {'Source':<20} {'Tok':<4} {'Delta':<6} {'Status':<10} {'Description'}")
    print(f"{'-'*8} {'-'*4} {'-'*20} {'-'*4} {'-'*6} {'-'*10} {'-'*20}")

    problems = []
    wins = []
    washes = []

    for neb, sources, desc in items:
        neb_tok = tok(neb)
        # Find the most common source equivalent
        best_src = min(sources, key=lambda s: tok(s))
        src_tok = tok(best_src)
        delta = neb_tok - src_tok

        if delta > 0:
            status = "WORSE"
            problems.append((neb, best_src, neb_tok, src_tok, desc))
        elif delta < 0:
            status = "BETTER"
            wins.append((neb, best_src, neb_tok, src_tok, desc))
        else:
            status = "wash"
            washes.append((neb, best_src, neb_tok, src_tok, desc))

        marker = "***" if delta > 0 else ("+++" if delta < 0 else "   ")
        print(f"{neb:<8} {neb_tok:<4} {best_src:<20} {src_tok:<4} {delta:>+3}    {status:<10} {desc} {marker}")

    return problems, wins, washes


def find_single_token_alternatives(target_meaning: str, current: str, max_len=4):
    """Find short strings that are single tokens in cl100k_base."""
    # Check common short strings
    candidates = []
    chars = "abcdefghijklmnopqrstuvwxyz"
    # 1-char
    for a in chars:
        if tok(a) == 1:
            candidates.append(a)
    # 2-char
    for a in chars:
        for b in chars:
            s = a + b
            if tok(s) == 1:
                candidates.append(s)
    # 3-char
    for a in chars:
        for b in chars:
            for c in chars:
                s = a + b + c
                if tok(s) == 1 and len(s) <= max_len:
                    candidates.append(s)
    return candidates


def main():
    print("NEBULA TOKENIZER AUDIT")
    print(f"Tokenizer: cl100k_base (tiktoken)")
    print(f"Goal: Find keywords where Nebula costs MORE tokens than source")

    all_problems = []
    all_wins = []
    all_washes = []

    for name, items in [("KEYWORDS", KEYWORDS), ("BUILTINS", BUILTINS), ("METHOD ALIASES", METHOD_ALIASES)]:
        p, w, wash = audit_group(name, items)
        all_problems.extend(p)
        all_wins.extend(w)
        all_washes.extend(wash)

    print(f"\n{'=' * 70}")
    print(f" SUMMARY")
    print(f"{'=' * 70}")
    print(f"  BETTER (Nebula saves tokens): {len(all_wins)}")
    for neb, src, nt, st, desc in all_wins:
        print(f"    {neb} ({nt} tok) vs {src} ({st} tok) — {desc}")
    print(f"  WASH (same tokens):           {len(all_washes)}")
    print(f"  WORSE (Nebula costs more):    {len(all_problems)}")
    for neb, src, nt, st, desc in all_problems:
        print(f"    {neb} ({nt} tok) vs {src} ({st} tok) — {desc}")

    if all_problems:
        print(f"\n{'=' * 70}")
        print(f" PROBLEM KEYWORDS — ALTERNATIVES")
        print(f"{'=' * 70}")
        for neb, src, nt, st, desc in all_problems:
            tokens = tok_detail(neb)
            print(f"\n  {neb} -> [{', '.join(repr(t) for t in tokens)}] ({nt} tokens)")
            print(f"    vs {src} -> [{', '.join(repr(t) for t in tok_detail(src))}] ({st} tokens)")
            # Check if adding a character helps
            for suffix in "aeiournt":
                candidate = neb + suffix
                if tok(candidate) == 1:
                    print(f"    SUGGESTION: {candidate!r} is a SINGLE token")
            # Check prefix
            for prefix in "aeiournt":
                candidate = prefix + neb
                if tok(candidate) == 1:
                    print(f"    SUGGESTION: {candidate!r} is a SINGLE token")

    # Special analysis: what 2-3 letter strings are single tokens?
    print(f"\n{'=' * 70}")
    print(f" SINGLE-TOKEN SHORT STRINGS (useful for keyword design)")
    print(f"{'=' * 70}")

    # Check all current Nebula keywords
    neb_keywords = [item[0] for items in [KEYWORDS, BUILTINS, METHOD_ALIASES] for item in items]
    single = [k for k in neb_keywords if tok(k) == 1]
    multi = [k for k in neb_keywords if tok(k) > 1]
    print(f"  Already single-token ({len(single)}): {', '.join(single)}")
    print(f"  Multi-token ({len(multi)}): {', '.join(f'{k}({tok(k)})' for k in multi)}")


if __name__ == "__main__":
    main()
