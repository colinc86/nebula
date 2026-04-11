# Nebula Neural Refinement — Training Guide

Instructions for running Phases 5 and 6 of the Nebula 1.0 plan on a Linux box with a 3090.

## Prerequisites

- Linux with NVIDIA 3090 (24GB VRAM)
- Python 3.10+
- CUDA 11.8+ or 12.x
- Rust toolchain (for building the Nebula compiler)
- Go 1.21+ (for evaluation)
- ~20GB disk for model weights + training data

## Setup

```sh
# Clone the repo (or copy it over)
cd /path/to/nebula

# Build the compiler
cd compiler
cargo build --release
cd ..

# Create Python environment
python3 -m venv .venv
source .venv/bin/activate

# Install training dependencies
pip install -r bench/finetune/requirements.txt

# For unsloth (if pip install fails):
pip install "unsloth[colab-new] @ git+https://github.com/unslothai/unsloth.git"
```

## Phase 5: Neural Refinement

### What This Does

Trains a small model (Qwen-2.5-1.5B) to translate Nebula IL → idiomatic Python. The model learns the mapping between Nebula's canonical form and real Python code, handling the patterns that rule-based codegen misses (list comprehensions, Pythonic idioms, etc.).

### Step 1: Prepare Training Data (~30-60 minutes, CPU)

Downloads Python functions from StarCoderData (HuggingFace), runs each through `nebula from-python`, and produces (nebula, python) training pairs.

```sh
cd bench/finetune

# Generate 20K training pairs (good starting point)
python3 prepare_data.py all --count 20000 --output-dir data/

# Or do it step by step:
python3 prepare_data.py download --output data/raw_python.jsonl --count 20000
python3 prepare_data.py generate --input data/raw_python.jsonl --output data/pairs.jsonl
python3 prepare_data.py split --input data/pairs.jsonl --train data/train.jsonl --val data/val.jsonl
```

This produces:
- `data/train.jsonl` — ~18K training examples
- `data/val.jsonl` — ~2K validation examples

Each example is SFT format:
```json
{"messages": [
  {"role": "system", "content": "You are a code translator..."},
  {"role": "user", "content": "<nebula code>"},
  {"role": "assistant", "content": "<python code>"}
]}
```

**Scaling up:** For better results, increase `--count` to 50K or 100K. The bottleneck is CPU time running the Nebula compiler on each file, not download speed. At 50K, data prep takes ~2-3 hours.

### Step 2: Train (~2-4 hours on 3090)

```sh
python3 train.py \
    --data data/train.jsonl \
    --val data/val.jsonl \
    --output models/nebula-python-v1 \
    --model unsloth/Qwen2.5-1.5B \
    --epochs 3 \
    --batch-size 4 \
    --grad-accum 4 \
    --lr 2e-4 \
    --max-seq-len 2048 \
    --lora-r 16 \
    --lora-alpha 16
```

**What this does:**
- Loads Qwen-2.5-1.5B in 4-bit quantization (~4GB VRAM)
- Adds LoRA adapters (rank=16) to all attention + MLP layers
- Trains for 3 epochs with AdamW 8-bit optimizer
- Saves checkpoints every 500 steps
- Exports final model + GGUF for Ollama

**VRAM usage:** ~6-8GB. The 3090 has plenty of headroom.

**Tuning knobs:**
- More data → better results (diminishing returns past ~50K)
- More epochs → risk of overfitting (3 is a good default)
- Higher LoRA rank → more capacity but slower (16 is sweet spot)
- Lower learning rate → more stable but slower convergence

**Dry run** (verify config without training):
```sh
python3 train.py --data data/train.jsonl --dry-run
```

### Step 3: Evaluate

```sh
# Full HumanEval (164 problems)
python3 eval.py --model models/nebula-python-v1 --problems 164 --output data/eval_results.jsonl

# Quick test (10 problems)
python3 eval.py --model models/nebula-python-v1 --problems 10

# Compare against base model (no fine-tuning)
python3 eval.py --model unsloth/Qwen2.5-1.5B --problems 164 --output data/eval_base.jsonl
```

**What we're measuring:** Given Nebula IL as input, does the model produce Python that passes HumanEval unit tests? The metric is Pass@1.

**Expected results:**
- Base Qwen-2.5-1.5B on raw Python: ~30-40% Pass@1 (published baseline)
- Base Qwen-2.5-1.5B on Nebula input: likely ~5-15% (unfamiliar syntax)
- Fine-tuned on Nebula→Python: target ~25-35% (close to base Python performance)

If the fine-tuned model's Pass@1 on Nebula input matches the base model's Pass@1 on Python input, that proves Nebula doesn't lose information — the model can work in Nebula just as well as in Python.

### Step 4: Export for Ollama (optional)

The training script auto-exports GGUF. To load in Ollama:

```sh
ollama create nebula-python -f models/nebula-python-v1-gguf/Modelfile
ollama run nebula-python "Translate to Python: def fibonacci(n):\n    if n <= 1:\n        return n\n    a, b = 0, 1\n    for _ in range(2, n + 1):\n        a, b = b, a + b\n    return b"
```

### One-Command Pipeline

If you just want to run everything:

```sh
cd bench/finetune
bash run_training.sh
```

This does steps 1-3 automatically.

---

## Phase 6: Error Correction Pipeline

Phase 6 adds a post-generation verification loop. After the model produces Python from Nebula, we check it and retry on failure.

### What This Does

1. **Generate:** Model produces Python from Nebula input
2. **Test:** Run the Python against unit tests
3. **If fail:** Feed the error message back to the model with "fix this" prompt
4. **Repeat:** Up to 3 correction attempts
5. **Fallback:** If still failing, use the rule-based `nebula to-python` output

This is the same "verify and retry" pattern used by AlphaCode and CodeT.

### Implementation

Phase 6 doesn't require additional training — it wraps the Phase 5 model in a retry loop. Here's how to run it:

```sh
# After Phase 5 training is complete:
python3 eval_with_correction.py \
    --model models/nebula-python-v1 \
    --problems 164 \
    --max-retries 3 \
    --output data/eval_corrected.jsonl
```

### Building eval_with_correction.py

This script doesn't exist yet. Here's the design:

```
For each HumanEval problem:
  1. Convert Python prompt+solution to Nebula
  2. Generate Python from Nebula (attempt 1)
  3. Run unit tests
  4. If PASS → done
  5. If FAIL → build retry prompt:
     "Your code failed with this error: {error}
      Fix the code. Output only the corrected Python."
  6. Generate again (attempt 2)
  7. Run tests again
  8. Repeat up to max_retries
  9. If still failing → fallback to nebula to-python output
  10. Record: which attempt succeeded (or fallback)
```

**Expected improvement:** Published results from similar retry systems show +10-20% Pass@1 improvement from error correction. If Phase 5 achieves 30% Pass@1, Phase 6 should push it to 35-45%.

### AST Round-Trip Check (bonus verification)

An additional check: ingest the generated Python back into Nebula and compare to the original Nebula input. If they differ significantly, the translation may have semantic drift.

```sh
# For each generated Python file:
nebula from-python generated.py > re_ingested.neb
diff original.neb re_ingested.neb
```

Large diffs indicate the model changed the algorithm, not just the style. This catches subtle bugs that unit tests might miss.

---

## Summary of Expected Timeline

| Step | Time on 3090 | What |
|---|---|---|
| Setup | 10 min | Install deps, build compiler |
| Data prep (20K) | 30-60 min | Download + convert Python→Nebula pairs |
| Training | 2-4 hours | QLoRA fine-tune Qwen-2.5-1.5B |
| Evaluation | 20 min | Run HumanEval Pass@1 |
| Error correction | 30 min | Retry loop on failures |
| **Total** | **~4-6 hours** | |

For a larger dataset (50K examples), add ~2 hours to data prep and ~4 hours to training (~8-10 hours total).

## Files Reference

```
bench/finetune/
  prepare_data.py      Data pipeline: download → convert → split
  train.py             QLoRA training with unsloth
  eval.py              HumanEval evaluation
  run_training.sh      One-command pipeline (Steps 1-3)
  requirements.txt     Python dependencies
  data/                Generated training data (after running)
    raw_python.jsonl    Downloaded Python functions
    pairs.jsonl         (nebula, python) pairs
    train.jsonl         Training split
    val.jsonl           Validation split
  models/              Trained models (after running)
    nebula-python-v1/   LoRA adapters + tokenizer
    nebula-python-v1-gguf/  Quantized GGUF for Ollama
```

## Current Results (for context)

These are the results from Phases 1-4, completed before training:

**Token Compression (Phase 3):**

| Language | Compression |
|---|---|
| Rust | 20.7% |
| Swift | 20.6% |
| Kotlin | 20.6% |
| C | 19.2% |
| TypeScript | 18.9% |
| Go | 12.2% |
| JavaScript | 11.0% |
| Python | 3.2% |
| **Overall** | **16.2%** |

**InterTrans Validation (Phase 4):**

Python→Go translation accuracy (averaged over 2 runs):
- Direct LLM: 67.5% CA
- Nebula-mediated: 75.0% CA (+7.5pp)

The compression numbers are fixed — they come from the rule-based ingester and don't depend on training. The training (Phase 5) is about teaching a model to work in Nebula fluently, so it can leverage that compression.
