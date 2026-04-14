# Using `nebula-host-30b` — self-hosted Nebula MCP agent

This is the end-to-end setup for running the fine-tuned `nebula-host-30b` model as your host coding agent, driving the Nebula MCP server, with no API dependencies. Everything runs on a single RTX 3090 (or equivalent 24 GB GPU).

`nebula-host-30b` is a LoRA fine-tune of Qwen3-Coder-30B-A3B-Instruct (Apache 2.0), trained on 138 agent traces distilled from Qwen3-Coder-480B-A35B-Instruct (also Apache 2.0) driving the Nebula MCP. Teacher and student share a tokenizer, tool-call format, and architecture family, which keeps the distilled patterns native to the student at inference time. The refactor traces in the training set (47 of 138) use a system prompt that makes `patch_code(replace_all=true)` the mandatory pattern for rename tasks, so the fine-tune reliably uses whole-file replace sweeps instead of brute-forcing with many individual patches.

## What you're installing

```
┌─────────────────────────────┐
│   host_agent_runner.py      │   ← the agent loop (Python, stdlib only)
│   (your Mac / workstation)  │
└────────┬────────────────────┘
         │ /api/chat (Ollama, OpenAI-compatible)
         ▼
┌─────────────────────────────┐
│   nebula-host-30b @ Ollama  │   ← fine-tuned Qwen3-Coder-30B-A3B (Q4_K_M, 18 GB)
│   (Linux box with 3090)     │
└─────────────────────────────┘
         ▲
         │ JSON-RPC over stdio (MCP)
┌────────┴────────────────────┐
│   compiler/target/release/  │   ← Nebula MCP server (Rust)
│   nebula mcp                │
│   (same box as the runner)  │
└─────────────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│   nebula-8lang-7b @ Ollama  │   ← translator backend for write_code
│   (same box as the host)    │
└─────────────────────────────┘
```

Three local components:
1. **`host_agent_runner.py`** — the Python agent loop. Speaks Ollama's `/api/chat` with tools, dispatches tool calls to the Nebula MCP over stdio, handles runtime nudges.
2. **Nebula MCP server** — the Rust binary at `compiler/target/release/nebula mcp`. Serves `read_code`, `write_code`, `patch_code`, `search_code`, `translate_code` over stdio.
3. **Two Ollama models** on your GPU box:
   - `nebula-host-30b` — the fine-tuned agent (Qwen3-Coder-30B-A3B LoRA). Drives tool calls.
   - `nebula-8lang-7b` — the translator backend that the MCP's `write_code` tool calls into for idiomatic Nebula → target language generation.

## Hardware requirements

| Component | Minimum |
|---|---|
| GPU VRAM | 24 GB (RTX 3090, 4090, A5000) — holds Q4_K_M 30B + 32K context KV cache |
| Disk | ~40 GB for both Ollama models (18 + 15 GB + ~5 GB slack) |
| RAM | 16 GB (CPU-side, for runner + MCP server) |
| Network | local only — runner → Ollama is usually LAN |

No cloud costs. No API keys. Zero per-inference spend.

## One-time setup

### 1. Build the Nebula MCP server

On the same box that will run the runner (or anywhere that can spawn the binary):

```bash
git clone https://github.com/electrocampbell/nebula.git
cd nebula/compiler
cargo build --release
# Binary: ./target/release/nebula
```

### 2. Install the models into Ollama

Both models live on the same 3090 box and are served by Ollama (≥ 0.18).

**The translator (required by `write_code`):**

```bash
# On the GPU box
huggingface-cli download electrocampbell/nebula-8lang-7b-gguf nebula-8lang-7b.Q4_K_M.gguf --local-dir ~/models/
cat > ~/models/Modelfile.translator << 'MF'
FROM ./nebula-8lang-7b.Q4_K_M.gguf
PARAMETER num_ctx 4096
PARAMETER temperature 0.1
MF
cd ~/models && ollama create nebula-8lang-7b -f Modelfile.translator
```

**The host agent:**

```bash
huggingface-cli download electrocampbell/nebula-host-30b-gguf nebula-host-30b.Q4_K_M.gguf --local-dir ~/models/
huggingface-cli download electrocampbell/nebula-host-30b-gguf Modelfile --local-dir ~/models/
cd ~/models && ollama create nebula-host-30b -f Modelfile
```

The host Modelfile includes an explicit Go chat template that enables Ollama's tool-call capability for the fine-tuned model (the auto-import path drops the `general.basename` metadata that Ollama normally uses for detection).

Confirm both are loaded:

```bash
ollama list | grep nebula
# nebula-host-30b:latest     ... 18 GB
# nebula-8lang-7b:latest     ... 15 GB

curl -s http://localhost:11434/api/show -d '{"name":"nebula-host-30b"}' | \
  python3 -c 'import json,sys; d=json.load(sys.stdin); print("caps:", d["capabilities"])'
# caps: ['completion', 'tools']
```

If capabilities does not include `tools`, re-create with the Modelfile from the HF repo — the template is what flips Ollama's tool-call flag.

### 3. Configure the runner

The runner is at `bench/finetune/host_agent_runner.py`. It's pure stdlib Python (no pip install needed). Point it at your Ollama host:

```bash
export NEBULA_OLLAMA_HOST=http://192.168.68.94:11434   # or localhost if everything's on one box
```

## Running an agent session

Basic usage — implement stub functions in a Go package:

```bash
cd nebula
python3 bench/finetune/host_agent_runner.py \
    --model nebula-host-30b \
    --ollama-url $NEBULA_OLLAMA_HOST \
    --mcp-command "$(pwd)/compiler/target/release/nebula" \
    --task-dir /path/to/your/go/package \
    --prompt "Implement the functions marked TODO. Run go test ./... when done." \
    --verify "go test ./..." \
    --output /tmp/session.jsonl \
    --max-turns 30
```

The runner will:
1. Spawn the Nebula MCP server as a subprocess with `cwd=/path/to/your/go/package`
2. Ask Ollama to run `nebula-host-30b` with the MCP tools registered
3. Loop: model emits tool calls → runner dispatches to MCP → result fed back → repeat
4. Stop when the model emits a final message with no tool calls, or `--max-turns` is hit
5. Run the `--verify` command after the loop ends and record the result

Output is a JSONL trace at `--output` + a summary printed to stdout.

## Runtime nudges (the Path-B safety net)

The runner watches for two patterns that are known weak spots of open-weights agent models and injects a hint when they fire:

1. **≥3 individual `patch_code` calls to the same file without `replace_all=true`.** Injects a hint pointing at `replace_all=true` for renames. **Rarely fires on Phase 12** — the refactor discipline is now trained in, so the model usually reaches for `replace_all=true` on its own. The nudge remains as a safety net for task shapes outside the training distribution.

2. **≥2 identical failed `old_str` not-found errors on the same file.** Injects a hint reminding the model that `patch_code` expects native-language source (not the Nebula form that `read_code` returns for files > 100 lines) and suggesting a `bash cat` to see raw bytes. Prevents the stuck-loop failure mode where the model gives up after a few failed retries.

Both nudges are opt-in via the runner, not baked into the model. If you want to turn them off, comment out the `Path-B nudge injection` block in `host_agent_runner.py` around line 403.

Nudge stats appear in the summary:

```
=== Summary ===
  ...
  nudges:   replace_all=8  stuck=2
  ...
```

## Workload guidance

**Good fit for `nebula-host-30b`:**
- Small-file implementations (stub → impl)
- Single-file bugfixes with clear test signal
- Code exploration / "explain this package"
- Multi-file renames and refactors — Phase 12 training fixed the brute-force pattern; a 10-file rename runs as ~10 `patch_code(replace_all=true)` calls, not 50+ individual patches

**Not tested / unlikely to work well:**
- Speculative feature design ("add a new module for X")
- Tasks requiring long-range reasoning across hundreds of lines

## Troubleshooting

| Symptom | Fix |
|---|---|
| `does not support tools` error from Ollama | Re-create the model using the Modelfile from the HF repo. Ollama's auto-import path drops the chat template that enables tool capability. |
| Model emits Nebula syntax as `old_str` and gets `old_str not found` | The stuck-loop nudge should catch this after 2 retries. If it's still looping, bump the model's input to include a `bash cat` of the file before patching. |
| Refactor runs long (≥60 turns) | Unusual on Phase 12 — the refactor discipline is trained in. If it does happen, bump `--max-turns` and file an issue so we can add the failing task shape to the training corpus. |
| Slow inference | Check `curl http://OLLAMA_HOST/api/ps` — confirm `size_vram == size` (100% GPU, no CPU spill). If it's spilling, drop `num_ctx` in the Modelfile from 32768 to 16384 or 8192. |
| MCP server panics on a file | File the issue with the panicking input. The Rust ingester is solid but there are rare edge cases — every one found so far has been fixed in the tree. |

## See also

- `bench/mcp_manual_test/RESULTS.md` — the round-by-round benchmark history, including Round 11 which is the current self-hosted result
- `bench/finetune/MODELS.md` — the full Nebula model registry including `nebula-host-30b`
- `bench/results.md` — aggregate benchmark results across every phase of the project
