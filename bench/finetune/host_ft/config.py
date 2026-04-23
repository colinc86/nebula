"""Shared constants for the host-model fine-tune pipeline.

Imported by every `slices/slice*.py`, `qc.py`, `assemble.py`, `launch_host_ft.py`,
and `eval_host_ft.py`. Any path/name/prompt change goes here.
"""
from __future__ import annotations
from pathlib import Path

# ── Paths ────────────────────────────────────────────────────────────

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent.parent  # compiler/, bench/, etc.
NEBULA_BIN = REPO / "compiler" / "target" / "release" / "nebula"

BUILD = HERE / "build"
SLICE_FILES = {
    "translation": BUILD / "slice1_translation.jsonl",
    "instruction": BUILD / "slice2_instruction.jsonl",
    "toolcall": BUILD / "slice3_toolcalls.jsonl",
    "anchor": BUILD / "slice4_anchor.jsonl",
    "divergence": BUILD / "slice4_divergence.jsonl",
}
REJECTED_FILES = {
    name: BUILD / f"slice{i}_{name}.rejected.jsonl"
    for i, name in enumerate(SLICE_FILES.keys(), start=1)
}
FINAL = {
    "train": BUILD / "host_ft_v1.train.jsonl",
    "val": BUILD / "host_ft_v1.val.jsonl",
    "held_out": BUILD / "host_ft_v1.held_out.jsonl",
    "enriched_train": BUILD / "host_ft_v1.train.enriched.jsonl",
    "enriched_val": BUILD / "host_ft_v1.val.enriched.jsonl",
    "enriched_held_out": BUILD / "host_ft_v1.held_out.enriched.jsonl",
    "manifest": BUILD / "manifest.json",
}
JOBS_DIR = HERE / "jobs"
EVAL_RESULTS = HERE / "eval_results"

PROMPT_POOLS = HERE / "prompt_pools"

# ── Slice target counts ──────────────────────────────────────────────

TARGET_COUNTS = {
    "translation": 800,
    "instruction": 1500,
    "toolcall": 500,
    "anchor": 200,
    "divergence": 250,
}
HELD_OUT_TOTAL = 200   # stratified across slices

# ── Models ────────────────────────────────────────────────────────────

PILOT_MODEL = "gpt-4o-mini-2024-07-18"
FULL_MODEL = "gpt-4.1-2025-04-14"

# Sonnet version for slice-2 synthesis and eval-3 judge.
SONNET_MODEL = "claude-sonnet-4-6"

# Stock mini for slice-4 targets + eval-3 baseline.
STOCK_MINI = PILOT_MODEL

# ── System prompts (locked) ──────────────────────────────────────────

SYSTEM_PROMPT_NEBULA = (
    "You are a coding assistant that emits Nebula IL.\n\n"
    "Nebula is a compact, language-agnostic intermediate representation. "
    "Always respond in Nebula when producing code. When you invoke tools "
    "that read or write source files, pass Nebula-form code, not native "
    "source. The host runtime translates Nebula to the target native "
    "language (Python, Go, JavaScript, TypeScript, Swift, Kotlin, Rust, "
    "C) on its side.\n\n"
    "When the user asks a general coding question that does not involve "
    "producing code for their project (e.g. conceptual explanations, "
    "debugging discussion, terminology), answer as you would normally."
)

SYSTEM_PROMPT_ANCHOR = (
    "You are a helpful coding assistant. Answer conceptual and debugging "
    "questions in prose."
)

# ── Supported language metadata ──────────────────────────────────────

LANGUAGES = [
    ("python",     "py",    "Python"),
    ("javascript", "js",    "JavaScript"),
    ("typescript", "ts",    "TypeScript"),
    ("go",         "go",    "Go"),
    ("swift",      "swift", "Swift"),
    ("kotlin",     "kt",    "Kotlin"),
    ("rust",       "rs",    "Rust"),
    ("c",          "c",     "C"),
]
LANG_EXT = {canon: ext for canon, ext, _ in LANGUAGES}
LANG_DISPLAY = {canon: disp for canon, _, disp in LANGUAGES}

# ── QC bounds ────────────────────────────────────────────────────────

MIN_NEBULA_LEN = 20
MAX_NEBULA_LEN = 3000
MAX_MESSAGE_LEN = 6000
MAX_RESPONSE_LEN = 3500
MAX_TOTAL_LEN = 12000


def ensure_dirs() -> None:
    """Create all directories the pipeline writes into."""
    for p in [BUILD, JOBS_DIR, EVAL_RESULTS, PROMPT_POOLS]:
        p.mkdir(parents=True, exist_ok=True)
