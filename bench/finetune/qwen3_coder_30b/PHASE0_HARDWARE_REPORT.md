# Phase 0 — Hardware Viability Report (G0) ✅ PASS

**Goal:** Confirm `qwen3-coder:30b` runs cleanly on the 3090 (host: 192.168.68.94) before any training spend.

**Date:** 2026-04-12
**Hardware:** RTX 3090 24 GB on `linuxbox` (192.168.68.94)
**Ollama version:** 0.18.0
**Model tag:** `qwen3-coder:30b`
**Architecture family:** `qwen3moe`
**Parameter size:** 30.5B total / 3.3B active (MoE)
**Quantization:** Q4_K_M (default ollama tag)
**Disk size:** 19.53 GB

## Pass criteria

| Check | Target | Result | Pass? |
|---|---|---|---|
| GPU-only inference (no CPU spill) | 100% layers on GPU | `size_vram == size` at every probe | ✅ |
| Idle/loaded VRAM ≤ 24 GB | ≤ 24 GB | 19.53 GB at 8K ctx, 22.00 GB at 32K ctx | ✅ |
| Tokens/sec on small prompt | ≥ 30 tok/s | **162.9 tok/s** | ✅ (5.4× target) |
| Tokens/sec on long-context prompt | ≥ 30 tok/s | **120.7 tok/s** at 16.5K input | ✅ (4× target) |
| 32K context loads | yes | 22.00 GB VRAM, GPU-only | ✅ |
| Tool-call format parses cleanly | structured `tool_calls` field | parsed correctly when prompt commits | ✅ |
| Long-context generation succeeds | completes coherently | 16,532-token input → 32-token coherent reply in 8.6s | ✅ |

**Verdict: ✅ PASS — proceed to Phase 1.** The dreaded `ollama#10458` GPU-utilization issue does NOT affect this hardware. The model is healthy at full context, 100% on the GPU, and well above the speed target.

## Detailed measurements

### Probe 1 — short prompt (24 input tokens, 8K ctx)

```
prompt_eval_count: 24
prompt_eval_dur_ms: 147
eval_count:        75
eval_dur_ms:       460
tok/s:             162.9
total_dur_ms:      7,494
```

Output (first generation, code-quality smoke check):
```go
func reverseString(s string) string {
    runes := []rune(s)
    for i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
        runes[i], runes[j] = runes[j], runes[i]
    }
    return string(runes)
}
```

Idiomatic Go. The 7,494 ms total includes the cold model load (~6.9 s); subsequent generations are fast.

### Probe 2 — VRAM at 8K context (loaded)

```json
{
  "name": "qwen3-coder:30b",
  "size": 19530102912,        // 19.53 GB
  "size_vram": 19530102912,   // 19.53 GB
  "context_length": 8192,
  "details": {"family": "qwen3moe", "parameter_size": "30.5B", "quantization_level": "Q4_K_M"}
}
```

**`size == size_vram` → 100% on GPU. No CPU offload.**

### Probe 3 — VRAM at 32K context

```
size: 22.00 GB  vram: 22.00 GB  ctx: 32768  → GPU-only
```

KV cache at 32K adds ~2.47 GB on top of the base weights. Comfortable 2 GB headroom on a 24 GB 3090. Q5_K_M would be borderline at this context; Q4_K_M is the right choice.

### Probe 4 — long-context generation

```
input_tokens:  16,532
output_tokens: 32
wall_time:     8.6 s
output_tok/s:  120.7
final VRAM:    22.00 GB (still GPU-only)
```

Input was ~80 KB of "Lorem ipsum" filler. Model correctly identified the file as "primarily made of repetitive placeholder text (Lorem ipsum) with some additional comments and code-like structures, likely serving as a sample or template file." Long context works structurally AND the model reads the whole window (it didn't just hallucinate based on the question).

### Probe 5 — tool-call format

**First attempt** (ambiguous prompt: "Read the file /tmp/example.go and tell me what it does"): Model emitted Qwen XML in `content` (`<function=read_code><parameter=path>...`) and Ollama did NOT parse it into `tool_calls`. Hedging response, half-formed.

**Second attempt** (commit prompt: "call read_code with path /tmp/x.go"): Clean structured parse:

```json
{
  "id": "call_c33b84r9",
  "function": {
    "index": 0,
    "name": "read_code",
    "arguments": {"path": "/tmp/x.go"}
  }
}
```

`tool_calls` field populated correctly, `content` empty. **Conclusion:** Ollama 0.18.0 *does* parse Qwen3-Coder tool calls into the standard structured field when the model commits to a tool call. The parser fails when the model emits hedging prose with embedded XML.

**Implication for Phase 1:** the host_agent_runner needs to:
1. **Prefer** `message.tool_calls` (structured) when present.
2. **Fall back** to parsing Qwen XML out of `message.content` when the structured field is empty but XML markers (`<function=` / `<tool_call>`) are present in content.
3. Use a system prompt that nudges the model toward decisive tool use ("Call the appropriate tool. Do not narrate.") to maximize the structured-parse rate.

This is a soft cost (extra ~30 LOC in the runner) — not a blocker for the gate.

## Notes / surprises

- **Speed exceeds research projections** (162 tok/s vs cited ~70 tok/s). Likely because the cited number was for a cold prompt with full context; subsequent warm generations on a 3090 are faster than the public guides indicate.
- **`ollama#10458` does not apply.** The open issue about poor GPU utilization on Qwen3-30B-A3B does not reproduce on this exact setup. May have been fixed in 0.18.0 or may be specific to certain GPU/driver combos. Either way: not our problem.
- **Tool-call hedging** was the only soft finding. Solvable with prompt engineering and parser fallback, NOT a model defect.
- **Context budget for the runner:** at Q4_K_M with 32K context we have ~2 GB headroom. Plan to cap our agent prompts at ~24K tokens of context to leave a safety margin for KV cache fluctuation across turns.

## Next phase

Phase 1: Build `bench/finetune/host_agent_runner.py`, run the existing implement + refactor tasks with stock `qwen3-coder:30b` as the host agent, and capture the baseline numbers. The runner needs:

- Talk to ollama `/api/chat` with `tools=[...]` schema, `num_ctx=32768`, temperature 0.1
- Spawn the Nebula MCP server (`compiler/target/release/nebula mcp`) over stdio
- Speak JSON-RPC 2.0 to drive `tools/list` and `tools/call`
- Loop: chat → if tool_calls → dispatch → feed result back as `role:tool` message → repeat (cap at 30 turns)
- Robust to the structured-vs-XML parsing fallback noted above
- Log per-turn token stats from ollama's response usage fields
- Tabulate final wall clock + test pass rate
