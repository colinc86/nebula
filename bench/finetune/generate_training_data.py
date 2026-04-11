#!/usr/bin/env python3
"""Generate fine-tuning training data for Nebula.

Produces JSONL training pairs from Go source files in multiple categories:
1. Full file Go → Nebula translation
2. Full file Nebula → Go translation
3. Function-level Go → Nebula translation (smaller, more focused)
4. Nebula code editing tasks
5. Keyword drill pairs (targeted keyword substitution training)
6. Nebula examples from the examples/ directory

Usage:
    python3 generate_training_data.py --go-list files.txt --output training.jsonl
    python3 generate_training_data.py --auto --output training.jsonl
"""

import argparse
import json
import os
import random
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
REPO_ROOT = SCRIPT_DIR.parent.parent
NEBULA_BIN = REPO_ROOT / "compiler" / "target" / "debug" / "nebula"
EXAMPLES_DIR = REPO_ROOT / "examples"
CORPUS_DIR = SCRIPT_DIR.parent / "corpus"

SYSTEM_PROMPT = "You are an expert Nebula programmer. Nebula is a compact language: fn=func, rt=return, st=struct, en=enum, im=impl, tr=trait, mc=match/switch, fo=for, wl=while, lp=loop, lt=let, mt=mut, sf=self, el=else, tt=true, ff=false, bk=break, ct=continue, and/or/not=logical, ln()=len(), psh()=append(), prt()=println()."

# Keyword drill pairs — teach the model individual substitutions
KEYWORD_DRILLS = [
    ("func add(a, b int) int { return a + b }",
     "fn add(a: int, b: int) -> int { rt a + b }"),
    ("func main() { fmt.Println(\"hello\") }",
     "fn main() { prt(\"hello\") }"),
    ("if x > 0 { return true } else { return false }",
     "if x > 0 { rt tt } el { rt ff }"),
    ("for i, v := range items { fmt.Println(i, v) }",
     "fo i, v in items { prt(i, v) }"),
    ("for x > 0 { x-- }",
     "wl x > 0 { x -= 1 }"),
    ("for { break }",
     "lp { bk }"),
    ("switch x { case 1: return \"one\" default: return \"other\" }",
     "mc x { 1 => \"one\" _ => \"other\" }"),
    ("type Point struct { X float64; Y float64 }",
     "st Point { X: float64\n  Y: float64 }"),
    ("type Color int\nconst ( Red Color = iota; Green; Blue )",
     "en Color { Red\n  Green\n  Blue }"),
    ("func (p *Point) Distance() float64 { return math.Sqrt(p.X*p.X + p.Y*p.Y) }",
     "im Point {\n  fn Distance(sf) -> float64 { rt math.Sqrt(sf.X*sf.X + sf.Y*sf.Y) }\n}"),
    ("type Stringer interface { String() string }",
     "tr Stringer { fn String(sf) -> string }"),
    ("x := len(items)",
     "mt x = ln(items)"),
    ("items = append(items, newItem)",
     "items = psh(items, newItem)"),
    ("if err != nil { continue }",
     "if err != nil { ct }"),
    ("x := 42\nvar y string",
     "mt x = 42\nmt y = go_var(\"string\")"),
    ("self.count++",
     "sf.count += 1"),
    ("go func() { doWork() }()",
     "sp fn() { doWork() }()"),
    ("ch <- value",
     "ch.send(value)"),
    ("x := <-ch",
     "mt x = ch.recv()"),
    ("close(ch)",
     "ch.close()"),
    ("delete(m, key)",
     "m.remove(key)"),
    ("results, err := process(data)\nif err != nil { return nil, err }",
     "mt (results, err) = process(data)\nif err != nil { rt (nil, err) }"),
    ("x && y || !z",
     "x and y or not z"),
]


def run_nebula(cmd, input_file):
    try:
        result = subprocess.run(
            [str(NEBULA_BIN), cmd, str(input_file)],
            capture_output=True, text=True, timeout=30
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
    except (subprocess.TimeoutExpired, FileNotFoundError):
        pass
    return None


def make_chat(system, user, assistant):
    msgs = []
    if system:
        msgs.append({"role": "system", "content": system})
    msgs.append({"role": "user", "content": user})
    msgs.append({"role": "assistant", "content": assistant})
    return {"messages": msgs}


def extract_functions(source, lang_prefix=""):
    """Extract individual functions from source code."""
    functions = []
    lines = source.split('\n')
    i = 0
    while i < len(lines):
        line = lines[i].strip()
        # Detect function start
        starts_fn = (line.startswith('func ') or line.startswith('fn ') or
                     line.startswith('  fn ') or line.startswith('    fn '))
        if starts_fn and '{' in line:
            depth = 0
            start = i
            for j in range(i, len(lines)):
                depth += lines[j].count('{') - lines[j].count('}')
                if depth <= 0:
                    fn_text = '\n'.join(lines[start:j+1])
                    if len(fn_text) > 30 and len(fn_text) < 3000:
                        functions.append(fn_text)
                    i = j + 1
                    break
            else:
                i += 1
        else:
            i += 1
    return functions


def generate_from_go_file(go_file):
    """Generate all training pair types from a Go file."""
    pairs = []
    go_source = go_file.read_text()
    if len(go_source) < 100:
        return pairs

    neb_source = run_nebula("from-go", go_file)
    if not neb_source:
        return pairs

    # 1. Full file Go → Nebula
    if len(go_source) < 6000:  # skip very large files for full translation
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Translate this Go code to Nebula:\n\n```go\n{go_source}\n```",
            f"```nebula\n{neb_source}\n```"
        ))

        # 2. Full file Nebula → Go
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Translate this Nebula code to Go:\n\n```nebula\n{neb_source}\n```",
            f"```go\n{go_source}\n```"
        ))

    # 3. Function-level pairs
    go_fns = extract_functions(go_source)
    neb_fns = extract_functions(neb_source)

    for go_fn in go_fns[:3]:
        # Find corresponding Nebula function by name
        fn_name = ""
        for word in go_fn.split():
            if word.startswith("(") or word == "func":
                continue
            if "(" in word:
                fn_name = word.split("(")[0]
                break
            fn_name = word
            break

        matching_neb = None
        for nf in neb_fns:
            if fn_name and fn_name in nf:
                matching_neb = nf
                break

        if matching_neb:
            pairs.append(make_chat(
                SYSTEM_PROMPT,
                f"Translate this Go function to Nebula:\n\n```go\n{go_fn}\n```",
                f"```nebula\n{matching_neb}\n```"
            ))

    # 4. Editing tasks — "add a method" style
    for neb_fn in neb_fns[:2]:
        sig = neb_fn.split('{')[0].strip() if '{' in neb_fn else neb_fn[:60]
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Implement this Nebula function:\n\n```\n{sig}\n```",
            f"```nebula\n{neb_fn}\n```"
        ))

    return pairs


def generate_keyword_drills():
    """Generate focused keyword substitution training pairs."""
    pairs = []
    for go_snippet, neb_snippet in KEYWORD_DRILLS:
        # Go → Nebula
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Convert to Nebula: `{go_snippet}`",
            f"`{neb_snippet}`"
        ))
        # Nebula → Go
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Convert to Go: `{neb_snippet}`",
            f"`{go_snippet}`"
        ))
    # Repeat drills 3x with slightly different prompts for reinforcement
    for go_snippet, neb_snippet in KEYWORD_DRILLS:
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Rewrite in Nebula:\n```go\n{go_snippet}\n```",
            f"```nebula\n{neb_snippet}\n```"
        ))
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"What is the Nebula equivalent of this Go code?\n```go\n{go_snippet}\n```",
            f"```nebula\n{neb_snippet}\n```"
        ))
    return pairs


def generate_from_neb_examples():
    """Generate from existing .neb example files."""
    pairs = []
    if not EXAMPLES_DIR.exists():
        return pairs
    for neb_file in sorted(EXAMPLES_DIR.glob("*.neb")):
        neb_source = neb_file.read_text().strip()
        if not neb_source or len(neb_source) < 50:
            continue
        name = neb_file.stem
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Write a {name.replace('_', ' ')} program in Nebula.",
            f"```nebula\n{neb_source}\n```"
        ))

        # Function-level from examples
        fns = extract_functions(neb_source)
        for fn in fns[:2]:
            sig = fn.split('{')[0].strip()
            pairs.append(make_chat(
                SYSTEM_PROMPT,
                f"Write this Nebula function: `{sig}`",
                f"```nebula\n{fn}\n```"
            ))
    return pairs


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--go-dir", help="Directory of Go files")
    parser.add_argument("--go-list", help="File with Go file paths")
    parser.add_argument("--auto", action="store_true")
    parser.add_argument("--output", default="training.jsonl")
    parser.add_argument("--max-files", type=int, default=1000)
    args = parser.parse_args()

    if not NEBULA_BIN.exists():
        print(f"Error: {NEBULA_BIN} not found. Run: cd compiler && cargo build")
        sys.exit(1)

    all_pairs = []

    # Keyword drills (high priority — repeat for reinforcement)
    print("Generating keyword drills...")
    drills = generate_keyword_drills()
    all_pairs.extend(drills)
    print(f"  {len(drills)} keyword drill pairs")

    # Nebula examples
    print("Generating from Nebula examples...")
    examples = generate_from_neb_examples()
    all_pairs.extend(examples)
    print(f"  {len(examples)} example pairs")

    # Go files
    go_files = []
    if args.go_list:
        with open(args.go_list) as f:
            go_files = [Path(l.strip()) for l in f if l.strip()]
        go_files = [f for f in go_files if f.exists()][:args.max_files]
        print(f"Loaded {len(go_files)} Go files")
    elif args.go_dir:
        go_dir = Path(args.go_dir)
        go_files = sorted(go_dir.rglob("*.go"))[:args.max_files]
        print(f"Found {len(go_files)} Go files")
    elif args.auto:
        for f in [CORPUS_DIR / "bench.go", SCRIPT_DIR.parent / "challenge" / "main.go"]:
            if f.exists():
                go_files.append(f)

    for i, gf in enumerate(go_files):
        if (i + 1) % 50 == 0 or i == 0:
            print(f"  [{i+1}/{len(go_files)}] {gf.name}...")
        pairs = generate_from_go_file(gf)
        all_pairs.extend(pairs)

    # Shuffle for training diversity
    random.shuffle(all_pairs)

    out = Path(args.output)
    with open(out, 'w') as f:
        for pair in all_pairs:
            f.write(json.dumps(pair) + '\n')

    print(f"\nTotal: {len(all_pairs)} training pairs → {out}")
    print(f"Size: {out.stat().st_size:,} bytes")

    try:
        import tiktoken
        enc = tiktoken.get_encoding("cl100k_base")
        total = sum(len(enc.encode(json.dumps(p))) for p in all_pairs)
        print(f"Tokens: {total:,}")
        cost_3ep = total * 3 / 1_000_000 * 3.0
        print(f"Est. cost (3 epochs, gpt-4o-mini): ~${cost_3ep:.2f}")
    except ImportError:
        pass


if __name__ == "__main__":
    main()
