#!/usr/bin/env bash
# Deploy the 14B multi-task fine-tune (Qwen2.5-14B-Instruct base) to the
# 192.168.68.94 GPU box. Same pattern as deploy_combined.sh but for the
# 14B dense model, which is much smaller (~8 GB Q4_K_M) and gives us
# massive VRAM headroom on the 3090.
set -euo pipefail
HOST="colin@192.168.68.94"
LOCAL_TAR="${NEB_TAR:-$(dirname "$0")/downloads/ft-3b6d1f83-f0e0_merged.tar.zst}"
REMOTE_ROOT="/home/colin/ollama-native"
REMOTE_HF_DIR="${REMOTE_ROOT}/neb14b-merged"
REMOTE_GGUF_FP16="${REMOTE_ROOT}/nebula-combined-14b-f16.gguf"
REMOTE_GGUF_Q4="${REMOTE_ROOT}/nebula-combined-14b-q4_k_m.gguf"
MODEL_NAME="nebula-combined-14b"

if [[ ! -f "$LOCAL_TAR" ]]; then
    echo "error: $LOCAL_TAR not found — wait for download to finish" >&2
    exit 1
fi

echo "=== [0/6] unload the 30B model to free VRAM ==="
ssh "$HOST" "
export OLLAMA_HOST=127.0.0.1:11435
export LD_LIBRARY_PATH=$REMOTE_ROOT/lib/ollama/cuda_v13:$REMOTE_ROOT/lib/ollama
# Tell Ollama to stop keeping the 30B in memory (keep_alive=0 on an empty chat)
curl -s http://127.0.0.1:11435/api/generate -d '{\"model\":\"nebula-combined-30b\",\"keep_alive\":0}' >/dev/null 2>&1 || true
~/ollama-native/bin/ollama ps
"

echo
echo "=== [1/6] scp $(du -h \"$LOCAL_TAR\" | cut -f1) → $HOST ==="
ssh "$HOST" "rm -rf $REMOTE_HF_DIR $REMOTE_GGUF_FP16 $REMOTE_GGUF_Q4; mkdir -p $REMOTE_HF_DIR"
scp "$LOCAL_TAR" "$HOST:$REMOTE_ROOT/neb14b-merged.tar.zst"

echo
echo "=== [2/6] extract ==="
ssh "$HOST" "cd $REMOTE_ROOT && zstd -d -c neb14b-merged.tar.zst | tar -xf - -C neb14b-merged && ls -lh neb14b-merged | tail -15"

echo
echo "=== [3/6] convert HF → GGUF FP16 via llama.cpp convert_hf_to_gguf.py ==="
ssh "$HOST" "
cd /home/colin/llama.cpp-src
~/llama-convert-venv/bin/python3 convert_hf_to_gguf.py $REMOTE_HF_DIR --outfile $REMOTE_GGUF_FP16 --outtype f16 2>&1 | tail -15
ls -lh $REMOTE_GGUF_FP16
"

echo
echo "=== [4/6] quantize FP16 → Q4_K_M ==="
ssh "$HOST" "~/llama.cpp-bin/llama-b8797/llama-quantize $REMOTE_GGUF_FP16 $REMOTE_GGUF_Q4 Q4_K_M 2>&1 | tail -5; ls -lh $REMOTE_GGUF_Q4"

echo
echo "=== [5/6] create Ollama model from Q4 GGUF ==="
# Qwen2.5-14B-Instruct's native chat template is different from qwen3-coder's.
# Our combined training data is OpenAI-style messages + tool_calls; Ollama will
# route this through its default qwen2.5 template when RENDERER isn't set.
# We try PARSER qwen3-coder first (because the training data emits qwen3-coder
# XML tool calls); if parsing fails, drop the PARSER line and fall back to JSON
# tool calls (ollama's default).
ssh "$HOST" "
cat > /tmp/neb14b.modelfile <<EOF
FROM $REMOTE_GGUF_Q4

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
export LD_LIBRARY_PATH=$REMOTE_ROOT/lib/ollama/cuda_v13:$REMOTE_ROOT/lib/ollama
~/ollama-native/bin/ollama rm $MODEL_NAME 2>&1 | tail -1 || true
~/ollama-native/bin/ollama create $MODEL_NAME -f /tmp/neb14b.modelfile 2>&1 | tail -5
~/ollama-native/bin/ollama list | grep $MODEL_NAME
"

echo
echo "=== [6/6] smoke test ==="
ssh "$HOST" "
export OLLAMA_HOST=127.0.0.1:11435
export LD_LIBRARY_PATH=$REMOTE_ROOT/lib/ollama/cuda_v13:$REMOTE_ROOT/lib/ollama
echo 'Reply with the single word: ok' | ~/ollama-native/bin/ollama run $MODEL_NAME 2>&1 | tail -3
~/ollama-native/bin/ollama ps
nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader
"
