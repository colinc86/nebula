#!/bin/bash
# =============================================================================
# Nebula Neural Refinement — Together AI Training Pipeline
# =============================================================================
#
# Prerequisites:
#   - TOGETHER_API_KEY set in environment or in openai.env
#   - pip install together datasets tiktoken
#   - Nebula compiler built (cargo build --release)
#
# Usage:
#   cd bench/finetune
#   bash run_training.sh
#
# =============================================================================

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DATA_DIR="$SCRIPT_DIR/data"

echo "=== Nebula Neural Refinement — Together AI ==="
echo ""

# Load API key
if [ -z "${TOGETHER_API_KEY:-}" ]; then
    if [ -f "$REPO_ROOT/openai.env" ]; then
        export $(grep TOGETHER_API_KEY "$REPO_ROOT/openai.env" | tr -d '"')
        echo "Loaded TOGETHER_API_KEY from openai.env"
    else
        echo "Error: TOGETHER_API_KEY not set"
        exit 1
    fi
fi

# Step 0: Build compiler
echo "[Step 0] Building Nebula compiler..."
cd "$REPO_ROOT/compiler"
cargo build --release 2>&1 | tail -1
cd "$SCRIPT_DIR"
echo ""

# Step 1: Prepare training data
echo "[Step 1] Preparing training data..."
if [ -f "$DATA_DIR/train.jsonl" ] && [ -f "$DATA_DIR/val.jsonl" ]; then
    TRAIN_COUNT=$(wc -l < "$DATA_DIR/train.jsonl")
    VAL_COUNT=$(wc -l < "$DATA_DIR/val.jsonl")
    echo "  Using existing data: $TRAIN_COUNT train, $VAL_COUNT val"
else
    echo "  Downloading and processing 20K Python functions..."
    python3 prepare_data.py all --count 20000 --output-dir "$DATA_DIR"
fi
echo ""

# Step 2: Launch training on Together AI
echo "[Step 2] Launching fine-tuning on Together AI..."
echo "  Model: Qwen/Qwen2.5-1.5B"
echo "  Method: LoRA (rank=16)"
echo ""

python3 train.py \
    --data "$DATA_DIR/train.jsonl" \
    --val "$DATA_DIR/val.jsonl" \
    --model "Qwen/Qwen2.5-1.5B" \
    --epochs 3 \
    --batch-size 4 \
    --lr 2e-5 \
    --lora-r 16 \
    --lora-alpha 16 \
    --suffix "nebula-python" \
    --wait

echo ""
echo "=== Training Complete ==="
echo ""
echo "Check status:  python3 train.py --status"
echo "List jobs:     python3 train.py --list"
echo "Evaluate:      python3 eval.py --model <output-model-name> --problems 164"
