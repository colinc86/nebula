#!/usr/bin/env python3
"""
Nebula Token Compression Benchmark

Measures token compression across 8 languages using tiktoken cl100k_base
(the industry-standard tokenizer for GPT-4/Claude-class models).

Usage: python3 run_bench.py [--nebula PATH]
"""

import os
import sys
import subprocess
import datetime

try:
    import tiktoken
except ImportError:
    print("Installing tiktoken...", file=sys.stderr)
    subprocess.check_call([sys.executable, "-m", "pip", "install", "tiktoken", "-q"])
    import tiktoken

# ── Configuration ──

CORPUS_DIR = os.path.join(os.path.dirname(__file__), "corpus")
NEBULA_BIN = os.environ.get("NEBULA_BIN") or os.path.join(
    os.path.dirname(__file__), "..", "compiler", "target", "debug", "nebula"
)

LANGUAGES = [
    ("Python",     "bench.py",    "from-python"),
    ("JavaScript", "bench.js",    "from-js"),
    ("TypeScript", "bench.ts",    "from-ts"),
    ("Go",         "bench.go",    "from-go"),
    ("Swift",      "bench.swift", "from-swift"),
    ("Kotlin",     "bench.kt",    "from-kt"),
    ("Rust",       "bench.rs",    "from-rs"),
    ("C",          "bench.c",     "from-c"),
]


def count_tokens_fresh(text: str, enc) -> int:
    return len(enc.encode(text))


def run_nebula(command: str, filepath: str) -> str:
    result = subprocess.run(
        [NEBULA_BIN, command, filepath],
        capture_output=True, text=True, timeout=10
    )
    if result.returncode != 0:
        print(f"  WARNING: nebula {command} failed: {result.stderr.strip()}", file=sys.stderr)
        return ""
    return result.stdout


def main():
    enc = tiktoken.get_encoding("cl100k_base")

    if not os.path.isfile(NEBULA_BIN):
        print(f"Error: nebula binary not found at {NEBULA_BIN}", file=sys.stderr)
        print("Build with: cd compiler && cargo build", file=sys.stderr)
        sys.exit(1)

    results = []

    for lang_name, filename, command in LANGUAGES:
        filepath = os.path.join(CORPUS_DIR, filename)
        if not os.path.isfile(filepath):
            print(f"  SKIP: {filepath} not found", file=sys.stderr)
            continue

        # Read source
        with open(filepath) as f:
            source = f.read()

        # Convert to Nebula
        nebula = run_nebula(command, filepath)
        if not nebula:
            print(f"  SKIP: {lang_name} conversion produced empty output", file=sys.stderr)
            continue

        # Measure
        src_tokens = count_tokens_fresh(source, enc)
        neb_tokens = count_tokens_fresh(nebula, enc)
        src_bytes = len(source.encode("utf-8"))
        neb_bytes = len(nebula.encode("utf-8"))

        token_delta = (1 - neb_tokens / src_tokens) * 100 if src_tokens > 0 else 0
        byte_delta = (1 - neb_bytes / src_bytes) * 100 if src_bytes > 0 else 0

        src_lines = source.count("\n")
        neb_lines = nebula.count("\n")

        results.append({
            "lang": lang_name,
            "src_tokens": src_tokens,
            "neb_tokens": neb_tokens,
            "token_delta": token_delta,
            "src_bytes": src_bytes,
            "neb_bytes": neb_bytes,
            "byte_delta": byte_delta,
            "src_lines": src_lines,
            "neb_lines": neb_lines,
        })

    # ── Output ──

    now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M")
    report = []
    report.append("# Nebula Token Compression Benchmark")
    report.append(f"")
    report.append(f"**Date:** {now}")
    report.append(f"**Tokenizer:** cl100k_base (tiktoken)")
    report.append(f"**Corpus:** Task Manager (~100 lines per language, equivalent logic)")
    report.append(f"**Nebula binary:** `{NEBULA_BIN}`")
    report.append("")
    report.append("## Results")
    report.append("")
    report.append("| Language   | Src Tokens | Neb Tokens | Token +/- | Src Bytes | Neb Bytes | Byte +/- | Src Lines | Neb Lines |")
    report.append("|------------|-----------|-----------|----------|----------|----------|---------|----------|----------|")

    for r in results:
        tok_sign = "-" if r["token_delta"] > 0 else "+"
        byte_sign = "-" if r["byte_delta"] > 0 else "+"
        report.append(
            f"| {r['lang']:<10} | {r['src_tokens']:>9} | {r['neb_tokens']:>9} | "
            f"{tok_sign}{abs(r['token_delta']):>5.1f}%  | {r['src_bytes']:>8,} | {r['neb_bytes']:>8,} | "
            f"{byte_sign}{abs(r['byte_delta']):>5.1f}%  | {r['src_lines']:>8} | {r['neb_lines']:>8} |"
        )

    report.append("")
    report.append("## Summary")
    report.append("")

    if results:
        token_deltas = [r["token_delta"] for r in results]
        byte_deltas = [r["byte_delta"] for r in results]
        mean_token = sum(token_deltas) / len(token_deltas)
        mean_byte = sum(byte_deltas) / len(byte_deltas)
        best = max(results, key=lambda r: r["token_delta"])
        worst = min(results, key=lambda r: r["token_delta"])

        total_src_tokens = sum(r["src_tokens"] for r in results)
        total_neb_tokens = sum(r["neb_tokens"] for r in results)
        overall = (1 - total_neb_tokens / total_src_tokens) * 100

        report.append(f"- **Overall token compression:** {overall:+.1f}% ({total_src_tokens} -> {total_neb_tokens} tokens)")
        report.append(f"- **Mean token compression:** {mean_token:+.1f}%")
        report.append(f"- **Mean byte compression:** {mean_byte:+.1f}%")
        report.append(f"- **Best compression:** {best['lang']} ({best['token_delta']:+.1f}% tokens)")
        report.append(f"- **Least compression:** {worst['lang']} ({worst['token_delta']:+.1f}% tokens)")
        report.append("")
        report.append("### Interpretation")
        report.append("")
        report.append("Positive percentages = Nebula is smaller (compression).")
        report.append("Negative percentages = Nebula is larger (expansion).")
        report.append("")
        report.append("These results represent the **pre-fine-tune baseline**. A model fine-tuned on Nebula")
        report.append("would eliminate the need for a syntax reference card (~500 tokens overhead), making")
        report.append("the effective compression even better for multi-file sessions.")

    text = "\n".join(report) + "\n"
    print(text)

    # Write to file
    results_path = os.path.join(os.path.dirname(__file__), "results.md")
    with open(results_path, "w") as f:
        f.write(text)
    print(f"\nResults written to {results_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
