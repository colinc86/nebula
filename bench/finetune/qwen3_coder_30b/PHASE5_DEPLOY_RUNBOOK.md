# Phase 5 — Validation Model Deploy Runbook

After Phase 4 finishes (the Together AI fine-tune job completes), follow these
steps to convert the merged checkpoint into an Ollama model and run it through
the same Phase 1 baseline tests for the G3 gate comparison.

## Prereqs

- Phase 4 job has completed; `phase4_job.json` has the job ID
- Together API key in `openai.env`
- SSH access to the Linux box (192.168.68.94) with ollama installed
- Ollama version ≥ 0.5 (for safetensors → GGUF auto-conversion)

## Step 1 — Download merged checkpoint from Together AI

Run on the Linux box (where there's bandwidth and disk for the ~60 GB safetensors download):

```bash
# Linux box, ~/work/nebula-host-validation
mkdir -p ~/work/nebula-host-validation && cd ~/work/nebula-host-validation

JOB_ID=$(cat /tmp/phase4_job.json | python3 -c "import json,sys; print(json.load(sys.stdin)['job_id'])")
TOGETHER_API_KEY=$(grep TOGETHER_API_KEY ~/openai.env | cut -d= -f2 | tr -d '"')

# Download the merged checkpoint as a tarball
python3 -c "
from together import Together
import os
os.environ['TOGETHER_API_KEY'] = '$TOGETHER_API_KEY'
client = Together()
resp = client.fine_tuning.content(ft_id='$JOB_ID', checkpoint='merged')
with open('model.tar.zst', 'wb') as f:
    for chunk in resp.iter_bytes(8192):
        f.write(chunk)
print('downloaded:', os.path.getsize('model.tar.zst'), 'bytes')
"

# Extract
tar --use-compress-program=unzstd -xf model.tar.zst
ls -la merged/
```

## Step 2 — Build the Modelfile + create the ollama model

```bash
# Still in ~/work/nebula-host-validation
cp ~/nebula/bench/finetune/qwen3_coder_30b/Modelfile.template ./Modelfile
# Edit `FROM ./merged` line if the extracted dir is named differently

ollama create nebula-host-validation -f Modelfile
# Ollama will auto-convert safetensors → GGUF Q4_K_M (10-30 minutes for 30B)
```

If ollama's safetensors importer chokes on the Qwen3 MoE format (it's relatively
new), fall back to llama.cpp:

```bash
git clone https://github.com/ggerganov/llama.cpp /tmp/llama.cpp
cd /tmp/llama.cpp && pip3 install -r requirements.txt && make -j

python3 convert_hf_to_gguf.py ~/work/nebula-host-validation/merged \
    --outfile ~/work/nebula-host-validation/model-bf16.gguf

./llama-quantize ~/work/nebula-host-validation/model-bf16.gguf \
    ~/work/nebula-host-validation/model-Q4_K_M.gguf Q4_K_M

# Then update the Modelfile to point at the gguf file:
#   FROM ./model-Q4_K_M.gguf
ollama create nebula-host-validation -f ~/work/nebula-host-validation/Modelfile
```

## Step 3 — Verify the new model loads on the 3090

```bash
# From anywhere
curl -s http://192.168.68.94:11434/api/show -d '{"name":"nebula-host-validation"}' | python3 -m json.tool | head -20

# Quick generation smoke test
curl -s http://192.168.68.94:11434/api/generate \
    -d '{"model":"nebula-host-validation","prompt":"hello","stream":false,"options":{"num_ctx":32768,"num_predict":16}}' \
    | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['response'])"

# Confirm GPU-only
curl -s http://192.168.68.94:11434/api/ps | python3 -c "
import json,sys
m = json.load(sys.stdin)['models'][0]
print(f\"vram={m['size_vram']/1e9:.1f}GB / size={m['size']/1e9:.1f}GB → {'GPU-only' if m['size_vram']==m['size'] else 'SPILL'}\")
"
```

## Step 4 — Run Phase 1 tests with the new model (G3 gate)

From the Mac (or wherever the runner lives):

```bash
cd ~/Local\ Documents/Programming/Misc/nebula

# Implement task
rm -rf /tmp/q5_implement && cp -R bench/mcp_manual_test/template /tmp/q5_implement
python3 bench/finetune/host_agent_runner.py \
    --model nebula-host-validation \
    --ollama-url http://192.168.68.94:11434 \
    --mcp-command "$(pwd)/compiler/target/release/nebula" \
    --task-dir /tmp/q5_implement \
    --prompt-file bench/mcp_manual_test/prompt.txt \
    --verify "go test ./..." \
    --output /tmp/q5_implement.jsonl \
    --max-turns 30

# Refactor task
rm -rf /tmp/q5_refactor && cp -R bench/mcp_manual_test/refactor_template /tmp/q5_refactor
python3 bench/finetune/host_agent_runner.py \
    --model nebula-host-validation \
    --ollama-url http://192.168.68.94:11434 \
    --mcp-command "$(pwd)/compiler/target/release/nebula" \
    --task-dir /tmp/q5_refactor \
    --prompt-file bench/mcp_manual_test/refactor_prompt.txt \
    --verify "go build ./... && go test ./..." \
    --output /tmp/q5_refactor.jsonl \
    --max-turns 50
```

## Step 5 — G3 gate decision

Tabulate the new numbers against Phase 1 stock-Qwen baseline. **The validation
model must show ≥10% improvement on at least one metric** (turn count, tool-call
count, or wall clock) to pass G3.

Most-likely improvement target: **refactor turn count**, since stock-Qwen used 47
turns and 25 individual patches in Phase 1, but the distilled traces from Sonnet
all use `replace_all=true`. The trained model should pick that pattern up.

| Metric | Phase 1 stock | Phase 5 trained | Δ |
|---|---:|---:|---:|
| Implement turns | 13 | _to fill_ | _to fill_ |
| Implement wall | 36 s | _to fill_ | _to fill_ |
| Implement test pass | 12/12 | _to fill_ | _to fill_ |
| Refactor turns | 47 | _to fill_ | _to fill_ |
| Refactor tool calls | 46 | _to fill_ | _to fill_ |
| Refactor test pass | 12/12 | _to fill_ | _to fill_ |

**If G3 passes** → proceed to Phase 6 (production training brief).
**If G3 fails** → debug. Likely culprits: wrong LoRA target_modules (router got trained), bad system prompt, training data too narrow, learning rate wrong. Re-run Phase 4 with adjustments.
