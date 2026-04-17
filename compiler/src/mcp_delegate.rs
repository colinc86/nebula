/// Delegate-coder backend for the `code_task` MCP tool.
///
/// Implements a minimal agent loop against Ollama's `/api/chat` (with tool-use).
/// The loop is tool-dispatch-driven: the caller supplies a closure that routes
/// tool-call-by-name to in-process handlers. This lets `mcp.rs::tool_code_task`
/// reuse `tool_read_code` / `tool_patch_code` / etc. without going back out over
/// JSON-RPC, so the whole delegate session runs inside one MCP request.
///
/// Design rationale and the broader "delegate-coder" architecture are in
/// `bench/loop_notes.md` iteration 1.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Default Ollama endpoint and model for delegate sessions. Overridable via
/// `NEBULA_DELEGATE_OLLAMA_URL` / `NEBULA_DELEGATE_MODEL` env vars.
const FALLBACK_DELEGATE_URL: &str = "http://localhost:11434";
const FALLBACK_DELEGATE_MODEL: &str = "qwen3-coder:30b";

pub fn default_delegate_url() -> String {
    std::env::var("NEBULA_DELEGATE_OLLAMA_URL")
        .or_else(|_| std::env::var("NEBULA_OLLAMA_URL"))
        .unwrap_or_else(|_| FALLBACK_DELEGATE_URL.to_string())
}

pub fn default_delegate_model() -> String {
    std::env::var("NEBULA_DELEGATE_MODEL")
        .unwrap_or_else(|_| FALLBACK_DELEGATE_MODEL.to_string())
}

/// Inference-side system prompt for the inner Nebula-native coder. Port of
/// `host_agent_runner.py::SYSTEM_PROMPT_PREFIX` plus a short "you were called
/// as a delegate" framing so the inner model knows to return a summary.
pub const DELEGATE_SYSTEM_PROMPT: &str = "\
You are an autonomous coding agent running inside a Nebula delegate session. \
You received a natural-language task from an outer host model that does NOT \
see your tool calls or the file contents — it only sees the final summary you \
return. Your job is to carry out the task end-to-end using the tools, verify \
it, and respond with a short plain-text summary of what you did and whether \
verification passed.\n\
\n\
Tool usage rules:\n\
- For large files, read_code returns source in Nebula form (a compact IL with \
`fn`/`rt`/`fo`/`:=`/indent blocks). Small files come back as raw native source.\n\
- patch_code ALWAYS matches against native source bytes (Go, Python, etc.), \
NEVER Nebula. If read_code showed Nebula, translate `old_str` mentally to the \
native syntax before calling patch_code.\n\
- For any rename across multiple files, use ONE patch_code call per affected \
file with `replace_all=true`. Prefer bulk mode (pass a directory `path` plus \
`glob`) for package-wide renames — one call covers the whole rename.\n\
- Prefer patch_code over write_code for any change smaller than a full rewrite.\n\
- After making changes, run the verify command via bash if the task needs it. \
A green build/test is the ONLY signal you're done.\n\
- When the task is complete, respond with a short plain-text summary (1-3 \
sentences): what you did, what files changed, whether verification passed. Stop \
calling tools.\n\
- If you cannot complete the task, say so explicitly in the summary and \
explain why. Do not pretend success.\
";

/// Configuration for a delegate session.
pub struct DelegateConfig {
    pub model: String,
    pub ollama_url: String,
    pub num_ctx: u32,
    pub temperature: f32,
    pub max_turns: usize,
    /// After the inner agent emits a final_message, run `verify_cmd`. If it
    /// fails, inject the failure output as a new user turn and let the inner
    /// agent keep working. This bounds how many times we do that before giving
    /// up. Set to 0 to disable in-loop repair.
    pub max_verify_retries: usize,
    pub extra_system_note: Option<String>,
}

impl DelegateConfig {
    pub fn with_defaults() -> Self {
        Self {
            model: default_delegate_model(),
            ollama_url: default_delegate_url(),
            // Configurable via NEBULA_DELEGATE_NUM_CTX. Default lowered from
            // 32768 → 8192 in Phase 20 to keep qwen3-coder:30b fully on GPU
            // when the sidecar (also a few GB of VRAM) is loaded concurrently.
            // The 8K floor handles ~95% of benchmark shapes; the 4 large ones
            // (refactor, large_file) exceed it and need an override.
            num_ctx: std::env::var("NEBULA_DELEGATE_NUM_CTX")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(8192),
            temperature: 0.2,
            max_turns: 60,
            // Raised from 5 → 8 in Phase 20: combined model legitimately
            // needs more repair rounds on complex shapes (e.g. large_file
            // hitting 49 turns). The stall detector is off by default so
            // retries can actually be used.
            max_verify_retries: 8,
            extra_system_note: None,
        }
    }
}

/// Per-turn inference metrics captured from Ollama's `/api/chat` response.
/// Phase 20 speed investigation — these feed the wall-clock breakdown that
/// decides whether a speed-focused fine-tune is worth chasing. Serialized
/// into the `code_task` tool result under `per_turn`.
#[derive(Default, serde::Serialize, Clone)]
pub struct TurnMetric {
    pub turn: usize,
    /// Tokens consumed by prompt eval (prefill). From Ollama `prompt_eval_count`.
    pub prompt_eval_count: u64,
    /// Prefill duration reported by Ollama, in ms. From `prompt_eval_duration` (ns).
    pub prompt_eval_ms: u64,
    /// Tokens generated this turn. From Ollama `eval_count`.
    pub eval_count: u64,
    /// Generation duration reported by Ollama, in ms. From `eval_duration` (ns).
    pub eval_ms: u64,
    /// Wall-clock we measured around the `/api/chat` HTTP call (includes
    /// network + queue + Ollama's internal overhead). Should be ≥ prompt_eval_ms
    /// + eval_ms; the delta is "Ollama overhead we can't attribute to inference."
    pub ollama_wall_ms: u64,
    /// Time spent dispatching non-bash tool calls produced by this turn.
    pub tool_dispatch_ms: u64,
    /// Time spent in bash (verify / test runners) triggered by this turn.
    pub bash_ms: u64,
}

/// Per-session metrics returned to the host model so it can decide whether
/// delegation paid. Serializes into the `code_task` tool result as JSON.
#[derive(Default, serde::Serialize)]
pub struct DelegateResult {
    pub final_message: String,
    pub turns_used: usize,
    pub tool_calls: usize,
    pub tool_calls_by_name: HashMap<String, usize>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub elapsed_ms: u64,
    /// Phase 20 instrumentation: per-turn breakdown of where time went.
    /// Enables the "is inner-loop wall-clock dominated by prefill, generation,
    /// tool round-trip, or verify?" question that gates a speed-focused fine-tune.
    pub per_turn: Vec<TurnMetric>,
    pub total_prefill_ms: u64,
    pub total_gen_ms: u64,
    pub total_ollama_wall_ms: u64,
    pub total_tool_dispatch_ms: u64,
    pub total_bash_ms: u64,
    pub stopped_reason: String,
    pub files_touched: Vec<String>,
    pub verify_passed: Option<bool>,
    pub verify_retries_used: usize,
    pub verify_output_tail: String,
    /// Non-fatal warnings to surface to the caller. Iteration 11 added the
    /// "zero files touched on a successful session" warning to catch silent
    /// non-completion where the inner agent returns a success summary but
    /// never actually edited anything (root-caused on refactor_iter10_e).
    pub warnings: Vec<String>,
}

/// Check whether a delegate-capable Ollama endpoint is reachable.
pub fn is_available() -> bool {
    ureq::get(&format!("{}/api/tags", default_delegate_url()))
        .timeout(Duration::from_secs(2))
        .call()
        .is_ok()
}

/// Single-shot Ollama call for translation error correction. No tool use, no
/// agent loop — just one prompt and one response. Returns the model's text output.
pub fn run_single_correction(prompt: &str) -> Result<String, String> {
    let config = DelegateConfig::with_defaults();
    let chat_url = format!("{}/api/chat", config.ollama_url);
    let payload = json!({
        "model": config.model,
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "stream": false,
        "options": {
            "num_ctx": config.num_ctx,
            "temperature": 0.1,
            "num_predict": 4096,
        },
    });
    let resp: Value = ureq::post(&chat_url)
        .timeout(Duration::from_secs(120))
        .send_json(&payload)
        .map_err(|e| format!("Ollama error: {}", e))?
        .into_json()
        .map_err(|e| format!("parse error: {}", e))?;
    resp.get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "No content in Ollama response".to_string())
}

/// Convert an MCP tool descriptor (shape emitted by `mcp.rs::tools_list`) into
/// the OpenAI-compatible tool schema that Ollama's `/api/chat` expects.
pub fn mcp_tool_to_openai(mcp_tool: &Value) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": mcp_tool.get("name").cloned().unwrap_or_else(|| json!("")),
            "description": mcp_tool.get("description").cloned().unwrap_or_else(|| json!("")),
            "parameters": mcp_tool.get("inputSchema").cloned().unwrap_or_else(|| json!({
                "type": "object", "properties": {}
            })),
        },
    })
}

/// Build a synthetic `bash` tool descriptor so the delegate can run verify
/// commands. The outer MCP server doesn't expose bash; we surface it only to
/// the inner agent.
fn bash_tool_schema() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "bash",
            "description": "Run a shell command in the working directory and return combined stdout+stderr. Use for go test, pytest, cargo check, etc.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Shell command to execute"}
                },
                "required": ["command"]
            }
        }
    })
}

/// Run a bash command for the delegate loop. Bounded, non-interactive.
fn run_bash(cmd: &str, cwd: &str, timeout_secs: u64) -> (bool, String) {
    if cmd.trim().is_empty() {
        return (false, "empty command".to_string());
    }
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .env("NEBULA_DELEGATE_BASH", "1")
        .output();
    match output {
        Ok(out) => {
            let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
            combined.push_str(&String::from_utf8_lossy(&out.stderr));
            let _ = timeout_secs; // TODO: enforce timeout via kill after Duration if needed
            let ok = out.status.success();
            let truncated = if combined.len() > 8000 {
                format!("{}\n... (truncated {} bytes)", &combined[..8000], combined.len() - 8000)
            } else {
                combined
            };
            (ok, format!("exit {}\n{}", out.status.code().unwrap_or(-1), truncated))
        }
        Err(e) => (false, format!("bash spawn error: {}", e)),
    }
}

/// The delegate agent loop. Calls Ollama `/api/chat` repeatedly, dispatches
/// tool calls via the supplied closure, and returns metrics plus the inner
/// agent's final natural-language summary.
pub fn run_delegate<F>(
    config: &DelegateConfig,
    user_instruction: &str,
    mcp_tools: &[Value],
    bash_cwd: Option<&str>,
    verify_cmd: Option<&str>,
    mut dispatch: F,
) -> Result<DelegateResult, String>
where
    F: FnMut(&str, &Value) -> Result<String, String>,
{
    // Build the tool schema list the inner agent sees.
    let mut tool_schemas: Vec<Value> = mcp_tools.iter().map(mcp_tool_to_openai).collect();
    if bash_cwd.is_some() {
        tool_schemas.push(bash_tool_schema());
    }

    // Compose system prompt.
    let mut system = DELEGATE_SYSTEM_PROMPT.to_string();
    if let Some(cwd) = bash_cwd {
        system.push_str(&format!(
            "\n\n== Working directory ==\n{}\n\
             When you call `bash`, the command runs from this directory.",
            cwd
        ));
    }
    if let Some(verify) = verify_cmd {
        system.push_str(&format!(
            "\n\n== Verification ==\n\
             The host has specified a verification command: `{}`\n\
             You MUST run this via bash and confirm it passes before writing your final summary. \
             If it fails, fix the cause and re-run.",
            verify
        ));
    }
    if let Some(note) = &config.extra_system_note {
        system.push_str("\n\n");
        system.push_str(note);
    }

    let mut messages: Vec<Value> = vec![
        json!({"role": "system", "content": system}),
        json!({"role": "user", "content": user_instruction}),
    ];

    let mut result = DelegateResult::default();
    let t0 = Instant::now();

    let chat_url = format!("{}/api/chat", config.ollama_url);

    // Track how many times in a row the inner agent has emitted the same
    // final_message prefix. Iteration 9 observed a failure mode where stock
    // qwen3-coder:30b on bugfix would repeat "Let me call patch_code now:"
    // 5 times in a row across retries. We bail at 3 consecutive matches —
    // enough to detect genuine stuck loops but lenient enough to ignore
    // temperature-sampling coincidences on the Phase 20 combined model, which
    // was being incorrectly flagged on the large_file shape at 2 matches.
    let mut last_stall_prefix: Option<String> = None;
    let mut stall_repeat_count: u32 = 0;

    // Outer loop bound: max_turns is the cap on Ollama /api/chat calls across
    // the whole session, including any repair rounds after verify failures.
    let mut turn = 0usize;
    loop {
        if turn >= config.max_turns {
            result.stopped_reason = "max_turns".to_string();
            result.elapsed_ms = t0.elapsed().as_millis() as u64;
            // Run verify one last time so the host knows the real state.
            if let Some(cmd) = verify_cmd {
                let cwd = bash_cwd.unwrap_or(".");
                let (ok, text) = run_bash(cmd, cwd, 180);
                result.verify_passed = Some(ok);
                result.verify_output_tail = tail(&text, 1200);
            }
            return Ok(result);
        }
        turn += 1;
        result.turns_used = turn;

        // Bump temperature on repair rounds. Temperature 0.2 has a strong
        // attractor toward "Let me X:" stall prefixes on bugfix; a higher
        // repair temperature shakes the model out of the stuck mode so it
        // actually emits a tool call. First round uses the configured
        // temperature; each subsequent retry adds 0.15, capped at 0.7.
        let turn_temperature = (config.temperature
            + 0.15 * result.verify_retries_used as f32)
            .min(0.7);

        let payload = json!({
            "model": config.model,
            "messages": &messages,
            "tools": &tool_schemas,
            "stream": false,
            "options": {
                "num_ctx": config.num_ctx,
                "temperature": turn_temperature,
                "num_predict": 2048,
            },
        });

        let ollama_start = Instant::now();
        let http = ureq::post(&chat_url)
            .timeout(Duration::from_secs(300))
            .send_json(&payload);

        let resp: Value = match http {
            Ok(r) => r.into_json().map_err(|e| format!("parse /api/chat response: {}", e))?,
            Err(e) => {
                result.stopped_reason = format!("ollama_error: {}", e);
                result.elapsed_ms = t0.elapsed().as_millis() as u64;
                return Err(format!(
                    "Delegate session failed: Ollama /api/chat error at {}: {}",
                    chat_url, e
                ));
            }
        };
        let ollama_wall_ms = ollama_start.elapsed().as_millis() as u64;

        // Phase 20 instrumentation: capture Ollama's own timing breakdown
        // (nanoseconds → ms). These fields may be absent on some Ollama
        // versions or non-Ollama OpenAI-compat shims; default to 0 if missing.
        let prompt_eval_count = resp.get("prompt_eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let eval_count = resp.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let prompt_eval_ms = resp
            .get("prompt_eval_duration")
            .and_then(|v| v.as_u64())
            .map(|ns| ns / 1_000_000)
            .unwrap_or(0);
        let eval_ms = resp
            .get("eval_duration")
            .and_then(|v| v.as_u64())
            .map(|ns| ns / 1_000_000)
            .unwrap_or(0);

        let mut turn_metric = TurnMetric {
            turn,
            prompt_eval_count,
            prompt_eval_ms,
            eval_count,
            eval_ms,
            ollama_wall_ms,
            tool_dispatch_ms: 0,
            bash_ms: 0,
        };

        result.input_tokens += prompt_eval_count;
        result.output_tokens += eval_count;
        result.total_prefill_ms += prompt_eval_ms;
        result.total_gen_ms += eval_ms;
        result.total_ollama_wall_ms += ollama_wall_ms;

        let msg = resp.get("message").cloned().unwrap_or_else(|| json!({}));
        let tool_calls = msg
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if tool_calls.is_empty() {
            // Inner agent emitted a final_message. Before returning, run the
            // verify command (if any). If it fails AND we have repair budget
            // left, inject the verify output as a new user turn and keep
            // the inner agent working — this bounds the +242% implement
            // regression we saw in iteration 5 where Sonnet had to bounce
            // through extra delegate calls to repair half-finished work.
            let final_text = msg
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Record the assistant final message so conversation state is
            // consistent if we decide to repair below.
            messages.push(msg);

            let verify = match verify_cmd {
                Some(cmd) => cmd,
                None => {
                    result.final_message = final_text;
                    result.stopped_reason = "final_message".to_string();
                    result.elapsed_ms = t0.elapsed().as_millis() as u64;
                    result.per_turn.push(turn_metric);
                    return Ok(result);
                }
            };

            let cwd = bash_cwd.unwrap_or(".");
            let verify_start = Instant::now();
            let (ok, vtext) = run_bash(verify, cwd, 180);
            let verify_ms = verify_start.elapsed().as_millis() as u64;
            turn_metric.bash_ms += verify_ms;
            result.total_bash_ms += verify_ms;
            result.verify_passed = Some(ok);
            result.verify_output_tail = tail(&vtext, 1200);

            if ok {
                // Silent non-completion guard: iteration 10 observed a case
                // where verify passed but the inner agent never actually
                // edited any files (returned a stall-prefix summary and the
                // initial code was already valid). Flag it so the caller can
                // tell the difference between "task done" and "task was
                // already done / verify command is too lenient".
                if result.files_touched.is_empty() {
                    result.warnings.push(format!(
                        "ZERO_FILES_MODIFIED: the inner agent reported success \
                         and `{}` passed, but no files were written or patched \
                         in this session. Either (a) the task was already \
                         satisfied before the session started, (b) the verify \
                         command is too lenient to detect the target state, or \
                         (c) the inner agent stalled without doing the work. \
                         If you expected file changes, treat this as a failure \
                         and retry with a stricter verify command.",
                        verify
                    ));
                }
                // Also warn if the final_message ends in a stall prefix —
                // trailing colon, "Let me...", "I'll...", "I need to...".
                let trimmed = final_text.trim();
                let tail20 = &trimmed[trimmed.len().saturating_sub(20)..];
                let is_stall = tail20.ends_with(':')
                    || trimmed.to_lowercase().contains("let me ")
                    || trimmed.to_lowercase().contains("i need to ")
                    || trimmed.to_lowercase().contains("i'll ")
                    || trimmed.to_lowercase().contains("now i'll ");
                if is_stall {
                    result.warnings.push(format!(
                        "STALL_PREFIX_IN_FINAL: the inner agent's final message \
                         ends like a queued action (\"{}\") rather than a \
                         complete summary. Verify passed but this is an \
                         unreliable success signal. If the task involved \
                         file changes, re-verify externally before trusting \
                         the result.",
                        tail(trimmed, 120)
                    ));
                }

                result.final_message = final_text;
                result.stopped_reason = "final_message".to_string();
                result.elapsed_ms = t0.elapsed().as_millis() as u64;
                result.per_turn.push(turn_metric);
                return Ok(result);
            }

            // Verify failed. Retry if we have budget.
            if result.verify_retries_used >= config.max_verify_retries {
                result.final_message = format!(
                    "{}\n\n[delegate] Verify command `{}` still failing after \
                     {} repair rounds. Returning with verify_passed=false so the \
                     host can take over.",
                    final_text, verify, result.verify_retries_used
                );
                result.stopped_reason = "verify_failed_max_retries".to_string();
                result.elapsed_ms = t0.elapsed().as_millis() as u64;
                result.per_turn.push(turn_metric);
                return Ok(result);
            }

            // Detect a genuine stuck-repetition: inner agent keeps emitting
            // the same non-tool-call prefix for N consecutive turns. Counter
            // is bumped on match, reset on any divergence. Bailing threshold
            // is 3 (see comment on stall_repeat_count init).
            let current_prefix = final_text.trim().to_string();
            let prefix_matches_last = last_stall_prefix
                .as_deref()
                .map(|prev| prev == current_prefix.as_str())
                .unwrap_or(false);
            if prefix_matches_last {
                stall_repeat_count += 1;
            } else {
                stall_repeat_count = 1;
            }
            let repeated_stall = stall_repeat_count >= 3;
            let logged_prefix = current_prefix.clone();
            last_stall_prefix = Some(current_prefix);
            // Phase 20 change: stall detection is now OFF by default. The
            // iteration-9 guard was designed for a stock qwen3-coder:30b
            // pathology ("Let me call patch_code now:" repeated 5 times on
            // bugfix) and fires on false positives on the Phase 20 combined
            // model, which legitimately emits similar prefixes during its
            // repair loop without being stuck. Opt in with
            // NEBULA_DELEGATE_ENABLE_STALL_DETECT=1 if a model regresses to
            // the iter-9 behavior.
            let log_stalls = std::env::var("NEBULA_DELEGATE_LOG_STALLS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            let enable_stall_detect = std::env::var("NEBULA_DELEGATE_ENABLE_STALL_DETECT")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            if log_stalls && repeated_stall {
                eprintln!(
                    "[mcp_delegate stall] turn={} retry={} repeated_stall=true prefix={:?}",
                    turn, result.verify_retries_used, tail(&logged_prefix, 400)
                );
            }
            if repeated_stall && result.verify_retries_used >= 1 && enable_stall_detect {
                result.final_message = format!(
                    "{}\n\n[delegate] Inner agent repeated the same stall \
                     message twice in a row without emitting a tool call. \
                     Early-exiting to avoid wasting retry budget. Verify is \
                     still failing; the host should take over with low-level \
                     tools.",
                    final_text,
                );
                result.stopped_reason = "verify_failed_repeat_stall".to_string();
                result.elapsed_ms = t0.elapsed().as_millis() as u64;
                result.per_turn.push(turn_metric);
                return Ok(result);
            }

            result.verify_retries_used += 1;

            // Detect a stalled "Let me X:" / "I need to Y:" prefix — these are
            // the inner agent queueing an action in its final message but never
            // actually calling the tool. Surface this explicitly so the next
            // turn doesn't just repeat the prefix.
            let stalled_prefix = {
                let trimmed = final_text.trim();
                let tail20 = &trimmed[trimmed.len().saturating_sub(20)..];
                tail20.ends_with(':')
                    || trimmed.to_lowercase().contains("let me ")
                    || trimmed.to_lowercase().contains("i need to ")
                    || trimmed.to_lowercase().contains("i'll ")
            };
            let stall_note = if stalled_prefix {
                format!(
                    "\n\nYour previous message ended with a queued action \
                     (\"{}\") but you did not actually call any tool. \
                     DO NOT WRITE ANOTHER PREFIX — call the tool right now with \
                     the actual arguments. If you said you'd patch a file, \
                     emit a patch_code tool_call this turn.",
                    tail(final_text.trim(), 160),
                )
            } else {
                String::new()
            };

            let repair_msg = format!(
                "The verification command `{}` FAILED with this output:\n\n\
                 ```\n{}\n```\n\n\
                 Diagnose the failure and fix it. Then run the verify command \
                 again via bash and confirm it passes before writing your final \
                 summary. Do NOT declare success until bash shows a zero exit \
                 code on the verify command.{}",
                verify,
                tail(&vtext, 2500),
                stall_note,
            );
            messages.push(json!({
                "role": "user",
                "content": repair_msg,
            }));
            // Continue the outer loop — inner agent gets another turn.
            result.per_turn.push(turn_metric);
            continue;
        }

        // Append the assistant turn (including its tool_calls) so Ollama can
        // track the conversation state on subsequent calls.
        messages.push(msg);

        for tc in &tool_calls {
            result.tool_calls += 1;
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            *result.tool_calls_by_name.entry(name.clone()).or_insert(0) += 1;

            let raw_args = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            let args = match raw_args {
                Value::String(s) => {
                    serde_json::from_str::<Value>(&s).unwrap_or_else(|_| json!({"_raw": s}))
                }
                v => v,
            };

            let (ok, tool_text) = if name.eq_ignore_ascii_case("bash") {
                let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                let cwd = bash_cwd.unwrap_or(".");
                let bash_start = Instant::now();
                let r = run_bash(cmd, cwd, 120);
                let elapsed = bash_start.elapsed().as_millis() as u64;
                turn_metric.bash_ms += elapsed;
                result.total_bash_ms += elapsed;
                r
            } else {
                // Strip Claude Code's mcp__nebula__ prefix if present.
                let bare = name
                    .strip_prefix("mcp__nebula__")
                    .unwrap_or(name.as_str());
                let dispatch_start = Instant::now();
                let r = match dispatch(bare, &args) {
                    Ok(s) => (true, s),
                    Err(e) => (false, format!("Error: {}", e)),
                };
                let elapsed = dispatch_start.elapsed().as_millis() as u64;
                turn_metric.tool_dispatch_ms += elapsed;
                result.total_tool_dispatch_ms += elapsed;
                r
            };

            // Track files touched (best-effort from args.path for write/patch).
            if ok && (name.contains("write_code") || name.contains("patch_code")) {
                if let Some(p) = args.get("path").and_then(|v| v.as_str()) {
                    if !result.files_touched.iter().any(|f| f == p) {
                        result.files_touched.push(p.to_string());
                    }
                }
            }

            let id = tc
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("call_x")
                .to_string();
            messages.push(json!({
                "role": "tool",
                "tool_call_id": id,
                "name": name,
                "content": tool_text,
            }));
        }

        // End of a tool-dispatch turn: commit the per-turn metric and loop.
        result.per_turn.push(turn_metric);
    }
}

/// Return the last `max_bytes` bytes of `s`, preceded by a truncation marker
/// if clipping occurred. Byte-safe — snaps forward to the next char boundary
/// so we don't split a multibyte codepoint.
fn tail(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut start = s.len() - max_bytes;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    format!("... ({} bytes truncated)\n{}", start, &s[start..])
}
