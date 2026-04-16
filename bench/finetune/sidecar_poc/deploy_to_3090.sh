#!/usr/bin/env bash
# Deploy the merged Phase 20 sidecar checkpoint to the 192.168.68.94 box.
#
# Sequence:
#   1. scp the .tar.zst to ~/nebula-sidecar/
#   2. ssh: extract → ~/nebula-sidecar/merged/
#   3. ssh: write Modelfile FROM /home/colin/nebula-sidecar/merged
#   4. ssh: ollama create nebula-sidecar-7b --experimental -q q4_K_M
#   5. ssh: smoke-test ollama run "ok?"
set -euo pipefail
HOST="colin@192.168.68.94"
LOCAL_TAR="$(dirname "$0")/downloads/ft-3bfe9b1d-68d8_merged.tar.zst"
REMOTE_DIR="/home/colin/nebula-sidecar"
REMOTE_TAR="${REMOTE_DIR}/merged.tar.zst"
REMOTE_OUT="${REMOTE_DIR}/merged"
MODEL_NAME="nebula-sidecar-7b"

if [[ ! -f "$LOCAL_TAR" ]]; then
    echo "error: $LOCAL_TAR not found — wait for download to finish" >&2
    exit 1
fi

echo "=== [1/4] scp $(du -h "$LOCAL_TAR" | cut -f1) → $HOST:$REMOTE_TAR ==="
ssh "$HOST" "mkdir -p $REMOTE_DIR && rm -rf $REMOTE_OUT"
scp "$LOCAL_TAR" "$HOST:$REMOTE_TAR"

echo
echo "=== [2/4] extract on remote ==="
ssh "$HOST" "set -e; cd $REMOTE_DIR; mkdir -p merged; zstd -d -c merged.tar.zst | tar -xf - -C merged; ls -la merged | head"

echo
echo "=== [3/4] write Modelfile ==="
ssh "$HOST" "cat > $REMOTE_DIR/Modelfile <<'EOF'
FROM $REMOTE_OUT

PARAMETER temperature 0.05
PARAMETER num_ctx 16384

TEMPLATE \"\"\"{{ if .System }}<|im_start|>system
{{ .System }}<|im_end|>
{{ end }}{{ if .Prompt }}<|im_start|>user
{{ .Prompt }}<|im_end|>
{{ end }}<|im_start|>assistant
{{ .Response }}<|im_end|>
\"\"\"

PARAMETER stop \"<|im_start|>\"
PARAMETER stop \"<|im_end|>\"
EOF
cat $REMOTE_DIR/Modelfile"

echo
echo "=== [4/4] ollama create $MODEL_NAME (quantize q4_K_M, takes ~2 min) ==="
ssh "$HOST" "cd $REMOTE_DIR && ollama create $MODEL_NAME --experimental -q q4_K_M -f Modelfile"

echo
echo "=== Smoke test ==="
ssh "$HOST" "ollama list | grep $MODEL_NAME; echo; echo 'Quick generation test:'; echo 'Reply with the single word: ok' | ollama run $MODEL_NAME --hidethinking 2>/dev/null | head"
