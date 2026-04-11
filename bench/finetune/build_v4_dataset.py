#!/usr/bin/env python3
"""Build the v4 training dataset: v3 (clean Go→Nebula) + curated pairs (10x weighted).

The curated pairs are duplicated 10× so they carry proportionally more signal
during training, pushing the model toward compact idiomatic Nebula instead of
verbose Go transliterations.
"""

import json
import random
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent

V3_FILE = SCRIPT_DIR / "training_v3.jsonl"
CURATED_FILES = [
    SCRIPT_DIR / "curated_pairs.jsonl",
    SCRIPT_DIR / "curated_pairs_extra.jsonl",
]
OUTPUT = SCRIPT_DIR / "training_v4.jsonl"

# How many times to duplicate each curated pair
CURATED_WEIGHT = 10


def main():
    # Load v3
    with open(V3_FILE) as f:
        v3 = [line for line in f]
    print(f"Loaded {len(v3)} v3 pairs from {V3_FILE.name}")

    # Load curated
    curated = []
    for cf in CURATED_FILES:
        with open(cf) as f:
            lines = [line for line in f]
            curated.extend(lines)
        print(f"Loaded {len(lines)} pairs from {cf.name}")

    print(f"Total curated: {len(curated)} pairs")

    # Weight curated
    weighted_curated = curated * CURATED_WEIGHT
    print(f"Weighted curated ({CURATED_WEIGHT}×): {len(weighted_curated)} pairs")

    # Combine and shuffle
    all_pairs = v3 + weighted_curated
    print(f"Total pairs before shuffle: {len(all_pairs)}")

    random.seed(42)
    random.shuffle(all_pairs)

    # Write
    with open(OUTPUT, "w") as f:
        for line in all_pairs:
            f.write(line)

    print(f"\nWrote {len(all_pairs)} pairs → {OUTPUT.name}")
    print(f"Size: {OUTPUT.stat().st_size:,} bytes")

    # Token count
    try:
        import tiktoken
        enc = tiktoken.get_encoding("cl100k_base")
        total = 0
        for line in all_pairs:
            total += len(enc.encode(line))
        print(f"Tokens: {total:,}")
        print(f"Est. cost (1 epoch, Qwen 14B): ~${total / 1_000_000 * 0.50:.2f}")
    except ImportError:
        pass

    # Also write train/val split
    random.seed(43)
    shuffled = all_pairs.copy()
    random.shuffle(shuffled)
    split = int(len(shuffled) * 0.9)
    train = shuffled[:split]
    val = shuffled[split:]

    train_file = SCRIPT_DIR / "train_v4.jsonl"
    val_file = SCRIPT_DIR / "val_v4.jsonl"

    with open(train_file, "w") as f:
        f.writelines(train)
    with open(val_file, "w") as f:
        f.writelines(val)

    print(f"\nTrain: {len(train)} pairs → {train_file.name}")
    print(f"Val:   {len(val)} pairs → {val_file.name}")
    print(f"Train size: {train_file.stat().st_size / 1024 / 1024:.1f} MB")
    print(f"Val size:   {val_file.stat().st_size / 1024 / 1024:.1f} MB")


if __name__ == "__main__":
    main()
