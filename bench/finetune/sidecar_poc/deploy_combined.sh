#!/usr/bin/env bash
# Deploy the COMBINED Phase 20 fine-tune to the 192.168.68.94 GPU box.
# Same pattern as deploy_to_3090.sh but for the combined model, and this time
# the import path is HF safetensors → llama.cpp GGUF FP16 → llama-quantize
# Q4_K_M → native Ollama create from GGUF. (Direct safetensors import via
# `ollama create --experimental` is broken by the MLX-CUDA lib issue we hit
# on the 7B deploy.)
set -euo pipefail
HOST="colin@192.168.68.94"
LOCAL_TAR="${NEB_TAR:-$(dirname "$0")/downloads/ft-857bdaeb-89c8_merged.tar.zst}"
REMOTE_ROOT="/home/colin/ollama-native"
REMOTE_HF_DIR="${REMOTE_ROOT}/combined-merged"
REMOTE_GGUF_FP16="${REMOTE_ROOT}/nebula-combined-30b-f16.gguf"
REMOTE_GGUF_Q4="${REMOTE_ROOT}/nebula-combined-30b-q4_k_m.gguf"
MODEL_NAME="nebula-combined-30b"

if [[ ! -f "$LOCAL_TAR" ]]; then
    echo "error: $LOCAL_TAR not found — wait for download to finish" >&2
    exit 1
fi

echo "=== [1/5] scp $(du -h "$LOCAL_TAR" | cut -f1) → $HOST ==="
ssh "$HOST" "rm -rf $REMOTE_HF_DIR $REMOTE_GGUF_FP16 $REMOTE_GGUF_Q4; mkdir -p $REMOTE_HF_DIR"
scp "$LOCAL_TAR" "$HOST:$REMOTE_ROOT/combined-merged.tar.zst"

echo
echo "=== [2/5] extract ==="
ssh "$HOST" "cd $REMOTE_ROOT && zstd -d -c combined-merged.tar.zst | tar -xf - -C combined-merged && ls -lh combined-merged | tail -15"

echo
echo "=== [3/5] convert HF → GGUF FP16 via llama.cpp convert_hf_to_gguf.py ==="
ssh "$HOST" "
if [[ ! -d /home/colin/llama.cpp-src ]]; then
    git clone --depth 1 https://github.com/ggml-org/llama.cpp /home/colin/llama.cpp-src
fi
cd /home/colin/llama.cpp-src
# Install conversion deps in a venv (isolated, cheap)
if [[ ! -d ~/llama-convert-venv ]]; then
    python3 -m venv ~/llama-convert-venv
    ~/llama-convert-venv/bin/pip install --quiet numpy sentencepiece gguf 'transformers<5' 'torch>=2' 'safetensors>=0.4' 'protobuf<5'
fi
~/llama-convert-venv/bin/python3 convert_hf_to_gguf.py $REMOTE_HF_DIR --outfile $REMOTE_GGUF_FP16 --outtype f16 2>&1 | tail -15
ls -lh $REMOTE_GGUF_FP16
"

echo
echo "=== [4/5] quantize FP16 → Q4_K_M ==="
ssh "$HOST" "~/llama.cpp-bin/llama-b8797/llama-quantize $REMOTE_GGUF_FP16 $REMOTE_GGUF_Q4 Q4_K_M 2>&1 | tail -5; ls -lh $REMOTE_GGUF_Q4"

echo
echo "=== [5/5] create Ollama model from Q4 GGUF ==="
ssh "$HOST" "
cat > /tmp/combined.modelfile <<EOF
FROM $REMOTE_GGUF_Q4

TEMPLATE {{ .Prompt }}
RENDERER qwen3-coder
PARSER qwen3-coder

PARAMETER temperature 0.2
PARAMETER top_k 20
PARAMETER top_p 0.8
PARAMETER num_ctx 32768
PARAMETER stop <|im_start|>
PARAMETER stop <|im_end|>
PARAMETER stop <|endoftext|>
EOF
export OLLAMA_HOST=127.0.0.1:11435
export LD_LIBRARY_PATH=/home/colin/ollama-native/lib/ollama/cuda_v13:/home/colin/ollama-native/lib/ollama
~/ollama-native/bin/ollama rm $MODEL_NAME 2>&1 | tail -1 || true
~/ollama-native/bin/ollama create $MODEL_NAME -f /tmp/combined.modelfile 2>&1 | tail -5
~/ollama-native/bin/ollama list | grep $MODEL_NAME
"

echo
echo "=== Smoke test ==="
ssh "$HOST" "
export OLLAMA_HOST=127.0.0.1:11435
export LD_LIBRARY_PATH=/home/colin/ollama-native/lib/ollama/cuda_v13:/home/colin/ollama-native/lib/ollama
echo 'Reply with the single word: ok' | ~/ollama-native/bin/ollama run $MODEL_NAME 2>&1 | tail -3
~/ollama-native/bin/ollama ps
nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader
"
