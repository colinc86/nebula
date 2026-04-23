#!/usr/bin/env bash
# Full GPT-4.1 production fine-tune launch (Track 2 Phase 9).
#
# Prereqs:
#   - bench/finetune/host_ft/build/host_ft_v1.train.jsonl SHA256 is 89022cb656d148c8...
#     (verify with: shasum -a 256 bench/finetune/host_ft/build/host_ft_v1.train.jsonl)
#     Produced by the post-turbofish-drop assemble.py on 2026-04-20.
#   - OpenAI quota ≥ ~$55 on the configured project.
#
# Cost estimate: 2,064,056 train tokens × 2 epochs × $25/Mtok = ~$51.60.
#   v3 on gpt-4o-mini used this exact dataset; 4.1 FT pricing is ~8.3×
#   higher per token but capacity ceiling should clear eval-1's 95% bar
#   that mini plateaued at 93.3%.
#
# Hyperparams (from v3 pilot): n_epochs=2 (auto picks 3 and overfits).
#
# After launch finishes, run:
#   .venv/bin/python3 bench/finetune/host_ft/eval_host_ft.py \
#       --model "ft:gpt-4.1-2025-04-14:personal:nebulahost-prod-v1:<snapshot>" \
#       --run-id prod-v1

set -euo pipefail

cd "$(dirname "$0")/../../.."

# Sanity check: confirm we're running against the post-turbofish-drop dataset.
TRAIN_FILE="bench/finetune/host_ft/build/host_ft_v1.train.jsonl"
EXPECTED_SHA="89022cb656d148c821a8eab07a6e9a29839b4609d6fac945c0db21c16d4b43f5"
ACTUAL_SHA=$(shasum -a 256 "$TRAIN_FILE" | awk '{print $1}')
if [[ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]]; then
    echo "ERROR: train-file SHA mismatch." >&2
    echo "  expected: $EXPECTED_SHA" >&2
    echo "  actual:   $ACTUAL_SHA" >&2
    echo "Re-run assemble.py or update this script's expected hash." >&2
    exit 1
fi

PYTHONUNBUFFERED=1 .venv/bin/python3 -u bench/finetune/host_ft/launch_host_ft.py \
    --model gpt-4.1-2025-04-14 \
    --suffix nebulahost-prod-v1 \
    --epochs 2
