#!/usr/bin/env python3
"""Prepare multi-target training data: Nebula -> {Python, JS, TS, Go, Swift, Kotlin, Rust, C}.

Downloads functions from StarCoderData (HuggingFace bigcode/starcoderdata) for
each language, converts to Nebula via the compiler, and produces SFT training
pairs for multi-target code generation.

Usage:
    # Full pipeline for all languages:
    python3 prepare_multi_target.py all --count 10000 --output-dir data/multi_target \\
        --languages python,javascript,typescript,go,rust,c

    # Download only (e.g. specific languages):
    python3 prepare_multi_target.py download --count 10000 --output-dir data/multi_target \\
        --languages python,go
"""

import argparse
import json
import os
import random
import re
import subprocess
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
NEBULA_BIN = REPO_ROOT / "compiler" / "target" / "debug" / "nebula"

MIN_FUNC_SIZE = 50
MAX_FUNC_SIZE = 2000

# Language name -> (StarCoder data_dir, file extension, from-* subcommand)
LANG_CONFIG = {
    "python": ("python", ".py", "from-python"),
    "javascript": ("javascript", ".js", "from-js"),
    "typescript": ("typescript", ".ts", "from-ts"),
    "go": ("go", ".go", "from-go"),
    "swift": ("swift", ".swift", "from-swift"),
    "kotlin": ("kotlin", ".kt", "from-kotlin"),
    "rust": ("rust", ".rs", "from-rs"),
    "c": ("c", ".c", "from-c"),
}

# Pretty names for system prompts
LANG_DISPLAY = {
    "python": "Python",
    "javascript": "JavaScript",
    "typescript": "TypeScript",
    "go": "Go",
    "swift": "Swift",
    "kotlin": "Kotlin",
    "rust": "Rust",
    "c": "C",
}


def load_hf_token():
    """Load HF_TOKEN from openai.env."""
    env_path = REPO_ROOT / "openai.env"
    if not env_path.exists():
        return os.environ.get("HF_TOKEN")
    with open(env_path) as f:
        for line in f:
            line = line.strip()
            if line.startswith("HF_TOKEN="):
                token = line.split("=", 1)[1].strip().strip('"').strip("'")
                return token
    return os.environ.get("HF_TOKEN")


# ---------------------------------------------------------------------------
# Function extractors (per language)
# ---------------------------------------------------------------------------

def extract_python_functions(source):
    """Extract top-level function definitions from Python source."""
    functions = []
    lines = source.split("\n")
    current_func = []
    in_func = False
    func_indent = 0

    for line in lines:
        stripped = line.lstrip()
        indent = len(line) - len(stripped)

        if stripped.startswith("def ") and (not in_func or indent <= func_indent):
            if current_func:
                func_text = "\n".join(current_func).rstrip()
                if func_text:
                    functions.append(func_text)
            current_func = [line]
            in_func = True
            func_indent = indent
        elif in_func:
            if stripped and indent <= func_indent and not stripped.startswith("def "):
                func_text = "\n".join(current_func).rstrip()
                if func_text:
                    functions.append(func_text)
                current_func = []
                in_func = False
            else:
                current_func.append(line)

    if current_func:
        func_text = "\n".join(current_func).rstrip()
        if func_text:
            functions.append(func_text)

    return functions


def _extract_brace_block(lines, start_idx):
    """Given lines and an index where '{' appears, extract through matching '}'."""
    depth = 0
    block = []
    for i in range(start_idx, len(lines)):
        line = lines[i]
        block.append(line)
        depth += line.count("{") - line.count("}")
        if depth <= 0 and depth != 0 - 1:
            break
        if depth == 0:
            break
    return "\n".join(block)


def extract_js_functions(source):
    """Extract function definitions from JavaScript/TypeScript source.

    Looks for:
      - function name(...)
      - export function name(...)
      - async function name(...)
      - const/let/var name = (...) => { ... }
      - const/let/var name = function(...)
    """
    functions = []
    lines = source.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.lstrip()

        # Standard function declarations
        is_func = False
        if re.match(r"^(export\s+)?(async\s+)?function\s+", stripped):
            is_func = True
        elif re.match(r"^(export\s+)?(const|let|var)\s+\w+\s*=\s*(async\s+)?\(", stripped):
            # Arrow function or function expression
            is_func = True
        elif re.match(r"^(export\s+)?(const|let|var)\s+\w+\s*=\s*function", stripped):
            is_func = True

        if is_func and "{" in line:
            block = _extract_brace_block(lines, i)
            if block:
                functions.append(block)
                # Skip past the block
                block_lines = block.count("\n") + 1
                i += block_lines
                continue

        i += 1

    return functions


def extract_go_functions(source):
    """Extract function definitions from Go source (looks for 'func ')."""
    functions = []
    lines = source.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.lstrip()

        if stripped.startswith("func ") and "{" in line:
            block = _extract_brace_block(lines, i)
            if block:
                functions.append(block)
                block_lines = block.count("\n") + 1
                i += block_lines
                continue

        i += 1

    return functions


def extract_rust_functions(source):
    """Extract function definitions from Rust source (looks for 'fn '/'pub fn ')."""
    functions = []
    lines = source.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.lstrip()

        if (stripped.startswith("fn ") or stripped.startswith("pub fn ")) and "{" in line:
            block = _extract_brace_block(lines, i)
            if block:
                functions.append(block)
                block_lines = block.count("\n") + 1
                i += block_lines
                continue

        i += 1

    return functions


def extract_c_functions(source):
    """Extract function definitions from C source.

    Heuristic: line starts at column 0, contains a '(', is not a #include,
    typedef, struct, or forward declaration (no trailing semicolon before body).
    """
    functions = []
    lines = source.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]

        # Must start at column 0 (no leading whitespace)
        if line and not line[0].isspace() and "(" in line:
            stripped = line.strip()
            # Skip preprocessor, typedefs, structs, forward decls
            if (stripped.startswith("#include") or stripped.startswith("typedef") or
                    stripped.startswith("struct") or stripped.endswith(";")):
                i += 1
                continue

            # Look for opening brace on this line or next
            if "{" in line:
                block = _extract_brace_block(lines, i)
                if block:
                    functions.append(block)
                    block_lines = block.count("\n") + 1
                    i += block_lines
                    continue
            elif i + 1 < len(lines) and "{" in lines[i + 1]:
                # Opening brace on next line (common C style)
                block = _extract_brace_block(lines, i)
                if block:
                    # Include the declaration line too
                    full_block = line + "\n" + _extract_brace_block(lines, i + 1)
                    functions.append(full_block)
                    block_lines = full_block.count("\n") + 1
                    i += block_lines
                    continue

        i += 1

    return functions


def extract_swift_functions(source):
    """Extract function definitions from Swift source (looks for 'func ')."""
    functions = []
    lines = source.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.lstrip()

        if (stripped.startswith("func ") or stripped.startswith("public func ") or
                stripped.startswith("private func ") or stripped.startswith("internal func ") or
                stripped.startswith("static func ") or stripped.startswith("override func ") or
                stripped.startswith("mutating func ")) and "{" in line:
            block = _extract_brace_block(lines, i)
            if block:
                functions.append(block)
                block_lines = block.count("\n") + 1
                i += block_lines
                continue

        i += 1

    return functions


def extract_kotlin_functions(source):
    """Extract function definitions from Kotlin source (looks for 'fun ')."""
    functions = []
    lines = source.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.lstrip()

        if (stripped.startswith("fun ") or stripped.startswith("private fun ") or
                stripped.startswith("public fun ") or stripped.startswith("internal fun ") or
                stripped.startswith("override fun ") or stripped.startswith("suspend fun ")) and "{" in line:
            block = _extract_brace_block(lines, i)
            if block:
                functions.append(block)
                block_lines = block.count("\n") + 1
                i += block_lines
                continue

        i += 1

    return functions


EXTRACTORS = {
    "python": extract_python_functions,
    "javascript": extract_js_functions,
    "typescript": extract_js_functions,  # Reuse JS extractor for TS
    "go": extract_go_functions,
    "swift": extract_swift_functions,
    "kotlin": extract_kotlin_functions,
    "rust": extract_rust_functions,
    "c": extract_c_functions,
}


# ---------------------------------------------------------------------------
# Nebula conversion
# ---------------------------------------------------------------------------

def to_nebula(source_code, lang):
    """Convert source code to Nebula using the compiler."""
    _, ext, subcmd = LANG_CONFIG[lang]
    with tempfile.NamedTemporaryFile(suffix=ext, mode="w", delete=False) as f:
        f.write(source_code)
        tmp_path = f.name

    try:
        result = subprocess.run(
            [str(NEBULA_BIN), subcmd, tmp_path],
            capture_output=True, text=True, timeout=10,
        )
        if result.returncode != 0:
            return None
        nebula = result.stdout.strip()
        if not nebula or len(nebula) < 10:
            return None
        return nebula
    except (subprocess.TimeoutExpired, Exception):
        return None
    finally:
        os.unlink(tmp_path)


def is_valid_pair(source_code, nebula_code):
    """Check if a (nebula, source) pair is valid for training."""
    if nebula_code == source_code:
        return False
    if "// @fallback" in nebula_code or "ERROR" in nebula_code:
        return False
    if len(nebula_code) < 20 or len(nebula_code) > 3000:
        return False
    code_lines = [l for l in nebula_code.splitlines()
                  if l.strip() and not l.strip().startswith("//")]
    if len(code_lines) < 2:
        return False
    return True


# ---------------------------------------------------------------------------
# Download functions from StarCoderData (with CodeSearchNet fallback)
# ---------------------------------------------------------------------------

def download_functions(lang, output_path, count=10000):
    """Download functions for a single language."""
    hf_token = load_hf_token()
    extractor = EXTRACTORS[lang]
    data_dir = LANG_CONFIG[lang][0]
    collected = 0

    with open(output_path, "w") as f:
        # Primary source: StarCoderData
        print(f"  [{lang}] Trying StarCoderData ({data_dir})...")
        try:
            from datasets import load_dataset
            ds = load_dataset(
                "bigcode/starcoderdata",
                data_dir=data_dir,
                split="train",
                streaming=True,
                token=hf_token,
            )
            hf_count = 0
            for sample in ds:
                if collected >= count:
                    break
                content = sample.get("content", "")
                if not content or len(content) < MIN_FUNC_SIZE:
                    continue
                functions = extractor(content)
                for func in functions:
                    if MIN_FUNC_SIZE <= len(func) <= MAX_FUNC_SIZE:
                        f.write(json.dumps({"code": func, "lang": lang}) + "\n")
                        collected += 1
                        hf_count += 1
                        if collected >= count:
                            break
                if hf_count > 0 and hf_count % 1000 == 0:
                    print(f"    {hf_count} functions from StarCoderData ({collected} total)...")
            print(f"    {hf_count} functions from StarCoderData")
        except Exception as e:
            print(f"    StarCoderData unavailable: {e}")

        # Fallback: The Stack (for languages not in StarCoderData, e.g. Swift)
        if collected < count and lang in ("swift",):
            remaining = count - collected
            print(f"  [{lang}] Fallback: The Stack (need {remaining} more)...")
            try:
                from datasets import load_dataset
                ds = load_dataset(
                    "bigcode/the-stack",
                    data_dir=f"data/{lang}",
                    split="train",
                    streaming=True,
                    token=hf_token,
                )
                stack_count = 0
                for sample in ds:
                    if collected >= count:
                        break
                    content = sample.get("content", "")
                    if not content or len(content) < MIN_FUNC_SIZE:
                        continue
                    functions = extractor(content)
                    for func in functions:
                        if MIN_FUNC_SIZE <= len(func) <= MAX_FUNC_SIZE:
                            f.write(json.dumps({"code": func, "lang": lang}) + "\n")
                            collected += 1
                            stack_count += 1
                            if collected >= count:
                                break
                    if stack_count > 0 and stack_count % 1000 == 0:
                        print(f"    {stack_count} from The Stack ({collected} total)...")
                print(f"    {stack_count} functions from The Stack")
            except Exception as e:
                print(f"    The Stack unavailable: {e}")

        # Fallback: CodeSearchNet (supports python, javascript, go)
        if collected < count and lang in ("python", "javascript", "go"):
            remaining = count - collected
            print(f"  [{lang}] Fallback: CodeSearchNet (need {remaining} more)...")
            try:
                from datasets import load_dataset
                csn_lang = lang if lang != "javascript" else "javascript"
                ds = load_dataset(
                    "code_search_net",
                    csn_lang,
                    split="train",
                    streaming=True,
                    token=hf_token,
                )
                csn_count = 0
                for sample in ds:
                    if collected >= count:
                        break
                    code = sample.get("func_code_string", "") or sample.get("whole_func_string", "")
                    if not code:
                        continue
                    if MIN_FUNC_SIZE <= len(code) <= MAX_FUNC_SIZE:
                        f.write(json.dumps({"code": code, "lang": lang}) + "\n")
                        collected += 1
                        csn_count += 1
                        if csn_count % 1000 == 0:
                            print(f"    {csn_count} from CodeSearchNet ({collected} total)...")
                print(f"    {csn_count} functions from CodeSearchNet")
            except Exception as e:
                print(f"    CodeSearchNet unavailable: {e}")

    print(f"  [{lang}] Total: {collected} functions -> {output_path}")
    return collected


# ---------------------------------------------------------------------------
# Generate training pairs
# ---------------------------------------------------------------------------

def generate_pairs(lang, input_path, output_path):
    """Generate (nebula, source) training pairs for one language."""
    display = LANG_DISPLAY[lang]
    print(f"Generating {display} pairs from {input_path}...")

    with open(input_path) as f:
        items = [json.loads(line) for line in f]

    print(f"  {len(items)} {display} functions loaded")

    valid = 0
    skipped = 0

    system_prompt = (
        f"You are a code translator. Given code in Nebula (a universal intermediate "
        f"language), produce the equivalent idiomatic {display} code. "
        f"Output only the {display} code, no explanations."
    )

    with open(output_path, "w") as out:
        batch_size = 100
        for batch_start in range(0, len(items), batch_size):
            batch = items[batch_start:batch_start + batch_size]

            for item in batch:
                source_code = item["code"]
                nebula_code = to_nebula(source_code, lang)

                if nebula_code is None:
                    skipped += 1
                    continue

                if not is_valid_pair(source_code, nebula_code):
                    skipped += 1
                    continue

                pair = {
                    "messages": [
                        {"role": "system", "content": system_prompt},
                        {"role": "user", "content": nebula_code},
                        {"role": "assistant", "content": source_code},
                    ]
                }
                out.write(json.dumps(pair) + "\n")
                valid += 1

            processed = min(batch_start + batch_size, len(items))
            print(f"  [{lang}] Processed {processed}/{len(items)} | "
                  f"Valid: {valid} | Skipped: {skipped}")

    print(f"  [{lang}] Generated {valid} valid pairs (skipped {skipped}) -> {output_path}")
    return valid


# ---------------------------------------------------------------------------
# Merge, shuffle, split
# ---------------------------------------------------------------------------

def merge_and_split(lang_pair_files, output_dir):
    """Merge all language pair files, shuffle, and split 90/10."""
    all_lines = []
    for lang, path in lang_pair_files.items():
        if not os.path.exists(path):
            continue
        with open(path) as f:
            lines = f.readlines()
        print(f"  {lang}: {len(lines)} pairs")
        all_lines.extend(lines)

    print(f"Total: {len(all_lines)} pairs across {len(lang_pair_files)} languages")

    random.seed(42)
    random.shuffle(all_lines)

    val_count = max(1, int(len(all_lines) * 0.1))
    val_lines = all_lines[:val_count]
    train_lines = all_lines[val_count:]

    train_path = os.path.join(output_dir, "train.jsonl")
    val_path = os.path.join(output_dir, "val.jsonl")

    with open(train_path, "w") as f:
        f.writelines(train_lines)
    with open(val_path, "w") as f:
        f.writelines(val_lines)

    print(f"\nSplit: {len(train_lines)} train, {len(val_lines)} val")
    print(f"  train.jsonl: {os.path.getsize(train_path) / 1024 / 1024:.1f} MB")
    print(f"  val.jsonl:   {os.path.getsize(val_path) / 1024 / 1024:.1f} MB")
    return train_path, val_path


# ---------------------------------------------------------------------------
# Pipeline
# ---------------------------------------------------------------------------

def run_download(languages, count, output_dir):
    """Download functions for specified languages."""
    os.makedirs(output_dir, exist_ok=True)
    for lang in languages:
        raw_path = os.path.join(output_dir, f"raw_{lang}.jsonl")
        if os.path.exists(raw_path):
            existing = sum(1 for _ in open(raw_path))
            print(f"[{lang}] Using existing {raw_path} ({existing} functions)")
        else:
            download_functions(lang, raw_path, count=count)


def run_all(languages, count, output_dir):
    """Run the full pipeline: download -> generate pairs -> merge -> split."""
    os.makedirs(output_dir, exist_ok=True)

    # Step 1: Download
    print("=" * 60)
    print("Step 1: Download functions")
    print("=" * 60)
    for lang in languages:
        raw_path = os.path.join(output_dir, f"raw_{lang}.jsonl")
        if os.path.exists(raw_path):
            existing = sum(1 for _ in open(raw_path))
            print(f"[{lang}] Using existing {raw_path} ({existing} functions)")
        else:
            download_functions(lang, raw_path, count=count)

    # Step 2: Generate pairs
    print("\n" + "=" * 60)
    print("Step 2: Generate Nebula <-> source pairs")
    print("=" * 60)
    pair_files = {}
    for lang in languages:
        raw_path = os.path.join(output_dir, f"raw_{lang}.jsonl")
        pairs_path = os.path.join(output_dir, f"pairs_{lang}.jsonl")
        if not os.path.exists(raw_path):
            print(f"[{lang}] Skipping (no raw data)")
            continue
        generate_pairs(lang, raw_path, pairs_path)
        pair_files[lang] = pairs_path

    # Step 3: Merge and split
    print("\n" + "=" * 60)
    print("Step 3: Merge and split")
    print("=" * 60)
    train_path, val_path = merge_and_split(pair_files, output_dir)

    print(f"\nDone! Multi-target training data ready in {output_dir}/")
    print(f"  Languages: {', '.join(languages)}")
    print(f"  train.jsonl: {sum(1 for _ in open(train_path))} examples")
    print(f"  val.jsonl:   {sum(1 for _ in open(val_path))} examples")


def main():
    parser = argparse.ArgumentParser(
        description="Prepare multi-target Nebula training data"
    )
    sub = parser.add_subparsers(dest="command")

    # all: full pipeline
    al = sub.add_parser("all", help="Run full pipeline (download + generate + split)")
    al.add_argument("--count", type=int, default=10000,
                     help="Number of functions to download per language")
    al.add_argument("--output-dir", default="data/multi_target",
                     help="Output directory")
    al.add_argument("--languages", default="python,javascript,typescript,go,rust,c",
                     help="Comma-separated list of target languages")

    # download: just download
    dl = sub.add_parser("download", help="Download functions only")
    dl.add_argument("--count", type=int, default=10000)
    dl.add_argument("--output-dir", default="data/multi_target")
    dl.add_argument("--languages", default="python,javascript,typescript,go,rust,c")

    args = parser.parse_args()

    if args.command in ("all", "download"):
        languages = [l.strip() for l in args.languages.split(",")]
        for lang in languages:
            if lang not in LANG_CONFIG:
                print(f"Error: unsupported language '{lang}'. "
                      f"Supported: {', '.join(LANG_CONFIG.keys())}")
                return
        if args.command == "all":
            run_all(languages, args.count, args.output_dir)
        else:
            run_download(languages, args.count, args.output_dir)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
