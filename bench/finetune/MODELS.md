# Nebula Fine-Tuned Models

Model registry and training history. All models trained via Together AI with QLoRA.

## Model Registry

### Production Models

**Nebula→target translators** (used by the MCP `write_code` and `translate_code` tools):

| Model | Base | Data | HuggingFace |
|---|---|---|---|
| `nebula-8lang-1.5b` | Qwen-2.5-1.5B | [68K pairs](https://huggingface.co/datasets/electrocampbell/nebula-8lang-68k) | [electrocampbell/nebula-8lang-1.5b](https://huggingface.co/electrocampbell/nebula-8lang-1.5b) |
| `nebula-8lang-7b` | Qwen-2.5-7B | [68K pairs](https://huggingface.co/datasets/electrocampbell/nebula-8lang-68k) | [electrocampbell/nebula-8lang-7b](https://huggingface.co/electrocampbell/nebula-8lang-7b) |
| `nebula-8lang-14b` | Qwen-2.5-14B | [203K pairs (30K/lang, +multi-func)](https://huggingface.co/datasets/electrocampbell/nebula-8lang-203k) | [electrocampbell/nebula-8lang-14b](https://huggingface.co/electrocampbell/nebula-8lang-14b) (BF16 safetensors) · [electrocampbell/nebula-8lang-14b-gguf](https://huggingface.co/electrocampbell/nebula-8lang-14b-gguf) (Q4_K_M, production default) |

**`nebula-8lang-14b-q4` is the MCP `translate_code` / `write_code` default.** The BF16 safetensors release is 29 GB and CPU-spills on a 24 GB GPU; the Q4_K_M build is ~8.4 GB, fits in ~15 GB VRAM alongside Ollama's KV cache at 4K context, and produces semantically equivalent translations to the full-precision checkpoint. Verified against gdsp's `convolution.go` — the Q4 model preserves the 2× multiplier, function decomposition (`conv` + `conv_c`), correct slice bounds, and helper calls like `v_mul_ec` that the 7B translator collapses. The earlier "14B needs 48 GB+ GPU for full performance" guidance referred to BF16 *HumanEval evaluation* — not Q4_K_M inference, which runs cleanly on a 3090 when host-30b isn't co-located.

**Agent model** (host coding agent that drives the Nebula MCP — calls `read_code`, `patch_code`, etc., and lets the translator above handle the final code generation):

| Model | Base | Data | HuggingFace |
|---|---|---|---|
| `nebula-host-30b` | Qwen3-Coder-30B-A3B-Instruct | [138 distilled agent traces](https://huggingface.co/datasets/electrocampbell/nebula-host-30b-traces) (Qwen3-Coder-480B teacher through the Nebula MCP; 47 refactor traces use a `replace_all=true`-mandatory prompt variant) | [electrocampbell/nebula-host-30b-gguf](https://huggingface.co/electrocampbell/nebula-host-30b-gguf) |

`nebula-host-30b` is a LoRA fine-tune (r=16, attention-only) trained on 138 agent traces where **Qwen3-Coder-480B-A35B-Instruct** drove the Nebula MCP on implement/refactor/bugfix/explain tasks. Teacher chosen for license compatibility (Apache 2.0 permits distillation) and same-family tokenizer/tool-call/architecture match with the 30B-A3B student.

The current checkpoint is **Phase 12** — 47 of the 138 training traces are refactor tasks re-distilled with a system prompt that makes `patch_code(replace_all=true)` mandatory for rename tasks. This fixed a brute-force refactor failure mode in the original Phase 11 checkpoint (Round 9: 55 individual patches and 176 s wall clock for the 10-file `KVCache → SnapCache` rename). Phase 12 runs the same task in 14 `patch_code(replace_all=true)` calls and 69 s wall clock — 61% faster, 72% fewer output tokens, still 12/12 tests passing. Implement behavior is unchanged (91 of 138 traces are the Phase 11 implement/bugfix/explain/cross-lang traces verbatim). It runs on a single RTX 3090 at Q4_K_M (18 GB) via Ollama and is designed to replace `claude -p` for agentic coding sessions with no API cost.

Paired with `bench/finetune/host_agent_runner.py` (the Python agent loop with runtime nudges) it passes both the round-9 implement test (19 turns, 12/12 tests) and the round-9 refactor test (57 turns, 12/12 tests), where stock Qwen3-Coder regresses on refactor without the runner nudges. See `bench/finetune/qwen3_coder_30b/USAGE.md` for end-to-end setup.

**Combined agent model (Phase 20)** — multi-task fine-tune that serves as both the inner coding agent and the sidecar error corrector. Supersedes `nebula-host-30b` as the production default for `code_task`.

| Model | Base | Data | HuggingFace |
|---|---|---|---|
| `nebula-combined-30b` | Qwen3-Coder-30B-A3B-Instruct | [342 multi-task traces](https://huggingface.co/datasets/electrocampbell/nebula-combined-30b-traces) (agent traces + sidecar correction triples, 50/50 by count, 10% large-file) | [electrocampbell/nebula-combined-30b-gguf](https://huggingface.co/electrocampbell/nebula-combined-30b-gguf) |

`nebula-combined-30b` is a QLoRA fine-tune (r=16, α=32, lr=1e-4, batch=2, 3 epochs, all-linear) trained on 342 multi-task examples: 50% agent traces (distilled from Qwen3-Coder-480B teacher through the Nebula MCP) and 50% sidecar Nebula→native correction triples (synthesized rename edits on Go and Python files). The training set includes 10% large-file traces (>200 LoC, short sequences only — the "candB" dosage that preserves core performance while adding large-file exposure). Together AI job: `ft-cc054260-d68c`. Cost: ~$4.

Benchmark (Phase 20, 8 shapes, N=3 each, inner-loop wall-clock vs stock Qwen3-Coder-30B):
- 7-shape aggregate (excl. large_file): **−54.8%** mean wall-clock, 21/21 verify_passed
- 8-shape aggregate (incl. large_file at 1/3 pass): −34.4%, 22/24 verify_passed
- Per-shape: add_feature −67.5%, bugfix −46.2%, extract_helper −25.2%, port_language −33.3%, write_from_spec −78.8%, refactor −58.6%, docstring_pass −56.7%, large_file +2.4%

Runs on a single RTX 3090 at Q4_K_M (~22 GB) via native Ollama (port 11435). Designed to run as the `code_task` inner agent with `NEBULA_DELEGATE_MODEL=nebula-combined-30b`.

### Earlier Models (6 languages, superseded)

| Model | Base | Languages | Data | Together Job | Pass@1 (HumanEval) |
|---|---|---|---|---|---|
| `nebula-python` | Qwen-2.5-1.5B | Python only | 20K pairs | `ft-45cf5753-952a` | 48.8% raw, 78.7% corrected |
| `nebula-multi-1.5b` | Qwen-2.5-1.5B | 6 (no Swift/Kotlin) | 51K pairs | `ft-b673e13a-607f` | 42.7% raw |
| `nebula-multi-7b` | Qwen-2.5-7B | 6 (no Swift/Kotlin) | 51K pairs | `ft-d2de31f0-7f76` | 68.9% raw, 89.6% corrected |

## Training Configuration

All models use the same LoRA configuration:

```
method:        SFT (Supervised Fine-Tuning)
lora_r:        16
lora_alpha:    16
lora_dropout:  0.05
lora_modules:  all-linear
epochs:        3
```

| Model Size | Batch Size | Learning Rate |
|---|---|---|
| 1.5B | 8 | 2e-5 |
| 7B | 8 | 1e-5 |
| 14B | 8 | 1e-5 |

## Training Data

Data generated by `prepare_multi_target.py` from StarCoderData (HuggingFace) + The Stack (for Swift).

### Pipeline

1. Download functions from StarCoderData/The Stack (streaming)
2. Extract individual functions using per-language heuristics
3. Convert each function to Nebula via `nebula from-{lang}`
4. Validate pairs (skip trivial, error markers, too short/long)
5. Format as SFT training pairs with language-specific system prompts
6. Shuffle and split 90/10 train/val

### System Prompt Format

```
You are a code translator. Given code in Nebula (a universal intermediate
language), produce the equivalent idiomatic {Language} code. Output only
the {Language} code, no explanations.
```

### Data Sizes

| Dataset | Count/Lang | Total Train | Total Val | Disk |
|---|---|---|---|---|
| 6-lang (10K) | 10,000 | 51K | 5.7K | 55 MB |
| 8-lang (10K) | 10,000 | 68K | 7.5K | 72 MB |
| 8-lang (30K) | 30,000 | ~216K | ~24K | ~220 MB |

### Per-Language Yield (10K target, 8-lang dataset)

| Language | Source | Functions | Valid Pairs | Yield |
|---|---|---|---|---|
| Python | StarCoderData | 10,000 | 9,276 | 92.8% |
| JavaScript | StarCoderData | 10,000 | 9,411 | 94.1% |
| TypeScript | StarCoderData | 10,000 | 8,574 | 85.7% |
| Go | StarCoderData | 10,000 | 9,991 | 99.9% |
| Swift | The Stack | 10,000 | 9,256 | 92.6% |
| Kotlin | StarCoderData | 10,000 | 9,568 | 95.7% |
| Rust | StarCoderData | 10,000 | 9,942 | 99.4% |
| C | StarCoderData | 10,000 | 9,207 | 92.1% |

## Evaluation Results

### HumanEval (164 problems, Nebula→Python, Pass@1)

| Model | Raw | With Error Correction |
|---|---|---|
| Base Qwen-2.5-1.5B (no fine-tuning) | 1.2% | — |
| `nebula-python` (1.5B, Python only) | 48.8% | 78.7% |
| `nebula-multi-1.5b` (1.5B, 6 lang) | 42.7% | — |
| `nebula-multi-7b` (7B, 6 lang) | 68.9% | 89.6% |
| `nebula-8lang-1.5b` (1.5B, 8 lang) | 45.1% | 79.3% |
| `nebula-8lang-7b` (7B, 8 lang) | 67.7% | 88.4% |

### MBPP (500 problems, Nebula→Python, Pass@1)

| Model | Pass@1 |
|---|---|
| `nebula-multi-7b` (7B, 6 lang) | 55.4% |
| Published Qwen-2.5-7B on raw Python | ~55-65% |

## How to Download and Use

```bash
# 1. Download from Together AI (requires API key)
TOGETHER_API_KEY="..." python3 -c "
from together import Together
client = Together()
response = client.fine_tuning.content(ft_id='JOB_ID', checkpoint='merged')
with open('model.tar.zst', 'wb') as f:
    for chunk in response.iter_bytes(8192):
        f.write(chunk)
"

# 2. Extract
tar --use-compress-program=unzstd -xf model.tar.zst

# 3. Load into Ollama
ollama create nebula-8lang-7b -f Modelfile

# 4. Test
nebula translate --from rust --to python input.rs
```

## Together AI Job IDs (for reference)

```
ft-45cf5753-952a  →  Qwen2.5-1.5B-nebula-python (3-epoch, Python only)
ft-b673e13a-607f  →  Qwen2.5-1.5B-nebula-multi-1.5b (3-epoch, 6 lang)
ft-d2de31f0-7f76  →  Qwen2.5-7B-nebula-multi-7b (3-epoch, 6 lang)
ft-1f6f55cf-6d80  →  Qwen2.5-1.5B-nebula-8lang-1.5b (3-epoch, 8 lang)
ft-7f0e30f3-03bb  →  Qwen2.5-7B-nebula-8lang-7b (3-epoch, 8 lang)
```
