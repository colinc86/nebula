# Phase 5 — Linuxbox Runbook (Option B fallback)

**Context:** Together's `/endpoints` API hung on every call from my sandbox
(5-minute read timeouts, zero bytes received — looks like a service issue on
their side, not ours). Training, job retrieval, and chat completions all work
fine, so we're pivoting Phase 5 off Together's serverless-inference product and
onto a local ollama deploy on the linuxbox.

All you need to do is run **4 commands** on the linuxbox. Total disk used ~60 GB
for the merged safetensors + ~20 GB for the auto-converted GGUF Q4_K_M. Clean up
when we're done with the model.

## What the runbook does

1. Downloads the merged safetensors checkpoint from Together AI (our
   `ft-ff666e8a-0684` validation run)
2. Extracts it
3. Points ollama at it via a Modelfile
4. Ollama auto-converts safetensors → GGUF Q4_K_M and registers it as
   `nebula-host-validation`

Then I take over from my side and drive the benchmark.

## Prereqs on the linuxbox

- `python3` with the `together` SDK installed (`pip install together` works if
  not already present)
- ollama ≥ 0.5 (you already have 0.18)
- ~80 GB free disk
- The `.env` file or `TOGETHER_API_KEY` env var accessible

## Commands to run (on the linuxbox)

```bash
# ─── 1. Set up workspace + load API key ───
mkdir -p ~/work/nebula-host-validation && cd ~/work/nebula-host-validation
export TOGETHER_API_KEY="tgp_v1_..."   # paste from your .env

# ─── 2. Download the merged checkpoint tarball (~60 GB) ───
python3 - << 'PY'
import os
from together import Together
client = Together()
# Use 'merged' to get the full fine-tuned model (not just the LoRA adapter),
# since ollama's safetensors importer wants a full model directory.
resp = client.fine_tuning.download(
    id='ft-ff666e8a-0684',
    output='model.tar.zst',
    checkpoint_type='merged',
)
print('downloaded:', resp)
PY
# If the `download` method isn't in your SDK version, fall back to:
# python3 -c "
# from together import Together
# client = Together()
# stream = client.fine_tuning.content(ft_id='ft-ff666e8a-0684', checkpoint='merged')
# with open('model.tar.zst','wb') as f:
#     for chunk in stream.iter_bytes(8192):
#         f.write(chunk)
# print('done')
# "

# ─── 3. Extract ───
tar --use-compress-program=unzstd -xf model.tar.zst
ls -la   # you should see a directory with safetensors files

# If the extracted directory isn't literally `./merged`, rename/symlink it:
# mv <whatever_the_dir_is_called> merged

# ─── 4. Create the ollama model ───
cat > Modelfile << 'MF'
FROM ./merged

PARAMETER num_ctx 32768
PARAMETER temperature 0.1
PARAMETER stop "<|im_end|>"
PARAMETER stop "<|im_start|>"

SYSTEM """You are an autonomous coding agent. Call tools decisively. Prefer patch_code over write_code for small changes. Use replace_all=true for renames. After edits, run the verify command. When tests pass, respond with a brief summary and stop."""
MF

ollama create nebula-host-validation -f Modelfile
# This triggers safetensors → GGUF auto-conversion, ~10-30 min for 30B

# ─── 5. Smoke test ───
curl -s http://localhost:11434/api/generate \
  -d '{"model":"nebula-host-validation","prompt":"hello","stream":false,"options":{"num_ctx":32768,"num_predict":8}}' \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print('RESPONSE:', d.get('response'))"

# Confirm GPU-only
curl -s http://localhost:11434/api/ps | python3 -c "
import json,sys
m = json.load(sys.stdin)['models']
print(m[0]['name'], 'vram:', m[0]['size_vram']/1e9, 'GB', 'size:', m[0]['size']/1e9, 'GB')
"
```

## Tell me when you're done

Just reply with:

- `ready` (happy path), or
- the error output if any step fails

I'll immediately drive the Phase 5 benchmark from my side using the existing
`host_agent_runner.py` pointed at `nebula-host-validation` on
`http://192.168.68.94:11434`.

## Troubleshooting

- **`client.fine_tuning.download` not found** — you have an older Together SDK. Use the fallback `client.fine_tuning.content()` snippet in the comment above step 2.
- **`ollama create` fails with "unsupported architecture"** — ollama 0.18 may not auto-handle Qwen3 MoE safetensors. Fall back to llama.cpp: `git clone https://github.com/ggerganov/llama.cpp && cd llama.cpp && pip install -r requirements.txt && python convert_hf_to_gguf.py ../merged --outfile ../model.gguf && make llama-quantize && ./llama-quantize ../model.gguf ../model.Q4_K_M.gguf Q4_K_M`, then change `FROM ./merged` to `FROM ./model.Q4_K_M.gguf` in the Modelfile and retry.
- **Download hangs** — Together's API has been flaky for the last hour (that's why we're on this path instead of dedicated endpoints). If the download stalls, Ctrl-C and retry — the SDK does NOT resume, you'll re-start from 0.
