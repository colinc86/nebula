/// MCP (Model Context Protocol) server for Nebula.
///
/// Implements JSON-RPC 2.0 over stdio with three tools:
/// - read_code: Read source files translated to Nebula
/// - write_code: Write Nebula code translated to target language
/// - search_code: Search codebase with results in Nebula

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use crate::lang_detect;
use crate::mcp_delegate;
use crate::neural;

/// Files at or below this many lines are returned raw (uncompressed). Below this
/// threshold the MCP primer + tool-call overhead exceeds the compression saving,
/// AND the model's verification instinct (Bash cat to confirm raw bytes before
/// patching) wipes out any read-side win. 100 covers most "normal" source files;
/// large files where compression actually pays go through the Nebula path.
const SMALL_FILE_THRESHOLD: usize = 40;

/// Number of leading raw-source lines to prepend as a "native preview" header
/// when serving compressed Nebula. Lets the host model anchor on the literal
/// package/imports/style without needing a Bash cat to verify before patching.
const PREVIEW_LINES: usize = 5;

#[derive(Clone)]
struct CacheEntry {
    mtime: SystemTime,
    raw: String,
    nebula_with_comments: String,
}

fn source_cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Read a source file with mtime-keyed caching, returning (raw_source, nebula_with_comments).
/// On cache hit the file isn't re-stat'd beyond mtime; on miss it's read, ingested, and stored.
fn read_source_cached(path: &str) -> Result<(String, String), String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("Failed to stat '{}': {}", path, e))?;
    let mtime = metadata.modified()
        .map_err(|e| format!("Failed to read mtime for '{}': {}", path, e))?;

    if let Ok(cache) = source_cache().lock() {
        if let Some(entry) = cache.get(path) {
            if entry.mtime == mtime {
                return Ok((entry.raw.clone(), entry.nebula_with_comments.clone()));
            }
        }
    }

    let raw = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read '{}': {}", path, e))?;
    let lang = lang_detect::detect(path);
    let nebula = lang_detect::to_nebula_with_opts(&raw, lang, true);

    if let Ok(mut cache) = source_cache().lock() {
        cache.insert(path.to_string(), CacheEntry {
            mtime,
            raw: raw.clone(),
            nebula_with_comments: nebula.clone(),
        });
    }

    Ok((raw, nebula))
}

fn invalidate_cache(path: &str) {
    if let Ok(mut cache) = source_cache().lock() {
        cache.remove(path);
    }
}

const NEBULA_PRIMER: &str = "\
This server reads files in Nebula (a compact canonical form spanning Python, JS, \
TS, Go, Swift, Kotlin, Rust, C). Small files (≤100 lines) come back as raw source. \
Larger files come back as Nebula with a 5-line native preview header showing real \
syntax — that preview is enough to anchor your edits, no need to Bash-cat the file.\n\
\n\
PREFERRED WORKFLOW — `code_task`: describe your ENTIRE task in plain English \
in ONE call and let the inner Nebula-native coder handle read/edit/verify for \
you. Example: `code_task(instruction=\"add a TitleCase helper to strings_util.go \
and make go test ./... pass\", verify=\"go test ./...\")`. The delegate runs \
your verify command at the end of the session and returns `verify_passed: \
true/false` — this is authoritative, trust it, do not re-verify yourself. If \
verify_passed is true, the work is done and tested; respond to the user with a \
short summary and STOP calling tools. Calling `code_task` more than once per \
task is almost always wrong: the circuit breaker refuses calls past #5 in a \
single session. If `code_task` is unavailable (no inner model reachable), fall \
back to the low-level tools below.\n\
\n\
LOW-LEVEL WORKFLOW: read_code to view, patch_code to edit, write_code only for \
new files or full rewrites. patch_code is byte-level on the raw source bytes, \
so old_str / new_str must be in the file's NATIVE language (Go, Python, etc.) \
— NOT Nebula. Use replace_all=true for repetitive substitutions within one file.\n\
\n\
For package-wide renames across many files, use patch_code BULK mode in a \
SINGLE call: pass `path` as the directory and `glob` like '*.go'. It walks \
the directory and applies the replacement to every matching file — one tool \
call, not one per file. Prefer this over `bash sed -i` loops: same semantics, \
lower tool-call overhead, and you get a structured per-file report back.\n\
\n\
Nebula syntax (only needed for low-level workflow): `fn name(a: Int) -> Int:` \
indent body / `rt x` / `fo i in 0..n:` / `x := 5` immutable, `x = 5` mutable / \
`\"hi {name}\"` interpolation / `|x| x+1` lambdas / `xs.map { it.v }` trailing \
closures / `and or not ??  |>` / `True False None`. Both short (`fn rt fo`) and \
long (`def return for`) keyword forms are 1 token.\
";

/// Entry point: run the MCP server on stdin/stdout.
pub fn serve() {
    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let response = handle_request(&request);

        if let Some(resp) = response {
            let mut out = stdout.lock();
            let _ = serde_json::to_writer(&mut out, &resp);
            let _ = out.write_all(b"\n");
            let _ = out.flush();
        }
    }
}

fn handle_request(request: &Value) -> Option<Value> {
    let id = request.get("id");
    let method = request.get("method")?.as_str()?;

    match method {
        "initialize" => {
            Some(jsonrpc_response(id, json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "nebula",
                    "version": "0.2.0"
                },
                "instructions": NEBULA_PRIMER
            })))
        }

        "notifications/initialized" => None, // no response for notifications

        "tools/list" => {
            let all_tools = json!([
                    {
                        "name": "read_code",
                        "description": "Read a source file translated to Nebula for token efficiency. Returns code in Nebula's compressed syntax.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": {
                                    "type": "string",
                                    "description": "Absolute path to the file to read"
                                },
                                "offset": {
                                    "type": "integer",
                                    "description": "Start line (1-indexed, optional)"
                                },
                                "limit": {
                                    "type": "integer",
                                    "description": "Max lines to return (optional)"
                                }
                            },
                            "required": ["path"]
                        }
                    },
                    {
                        "name": "write_code",
                        "description": "Write Nebula code to a file, auto-translating to the file's native language based on extension.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": {
                                    "type": "string",
                                    "description": "Absolute path to write to"
                                },
                                "code": {
                                    "type": "string",
                                    "description": "Nebula source code to translate and write"
                                }
                            },
                            "required": ["path", "code"]
                        }
                    },
                    {
                        "name": "search_code",
                        "description": "Search codebase for a pattern, returning matching snippets translated to Nebula.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": {
                                    "type": "string",
                                    "description": "Search pattern (substring match)"
                                },
                                "path": {
                                    "type": "string",
                                    "description": "Directory to search (default: current directory)"
                                },
                                "glob": {
                                    "type": "string",
                                    "description": "File pattern filter, e.g. '*.py'"
                                }
                            },
                            "required": ["query"]
                        }
                    },
                    {
                        "name": "patch_code",
                        "description": "Edit source by replacing a snippet. Send `old_str` and `new_str` in the file's NATIVE LANGUAGE (not Nebula) — byte-level replace, not a round-trip. Single-file mode: pass `path` to a file; `old_str` must appear once unless `replace_all=true`. Bulk mode (multi-file rename in ONE call): pass `path` to a directory plus `glob` (e.g. '*.go'); applies the replacement to every matching file that contains `old_str`, implicitly replace_all. Bulk mode is the right tool for package-wide renames — one call instead of N.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": {
                                    "type": "string",
                                    "description": "Absolute path. File path in single-file mode, directory path in bulk mode (when `glob` is set)."
                                },
                                "old_str": {
                                    "type": "string",
                                    "description": "Existing snippet to replace, in the file's native language (Go, Python, etc.). Must match source byte-for-byte including indentation."
                                },
                                "new_str": {
                                    "type": "string",
                                    "description": "Replacement snippet, in the file's native language"
                                },
                                "replace_all": {
                                    "type": "boolean",
                                    "description": "Single-file mode only: if true, replace every occurrence; if false (default), require exactly one match. Ignored in bulk mode (always true)."
                                },
                                "glob": {
                                    "type": "string",
                                    "description": "Bulk mode: file pattern like '*.go' or '*.py'. When set, `path` must be a directory; the patch is applied to every matching file that contains `old_str`. One call covers the whole package."
                                }
                            },
                            "required": ["path", "old_str", "new_str"]
                        }
                    },
                    {
                        "name": "translate_code",
                        "description": "Translate code between languages via Nebula IL. Tries neural translation (Ollama) first, falls back to rule-based codegen.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "code": {
                                    "type": "string",
                                    "description": "Source code to translate"
                                },
                                "source_language": {
                                    "type": "string",
                                    "description": "Source language (e.g. 'python', 'javascript', 'go')"
                                },
                                "target_language": {
                                    "type": "string",
                                    "description": "Target language (e.g. 'python', 'javascript', 'go')"
                                }
                            },
                            "required": ["code", "source_language", "target_language"]
                        }
                    },
                    {
                        "name": "code_task",
                        "description": "Delegate an entire coding task to a Nebula-native inner agent in ONE call. Give it a complete natural-language instruction + a `verify` shell command (e.g. 'go test ./...' or 'pytest'). The inner agent reads/edits the files, runs verify, and auto-retries repairs up to 3 times if verify fails — all inside this single tool call. The returned JSON contains `verify_passed: true/false` which is authoritative: the delegate literally ran your verify command at the end of the session and the boolean reflects the real exit status. You do NOT need to re-verify after this tool returns — if `verify_passed: true`, the work is done and tested. Re-verifying wastes tokens. Call this ONCE per task with a complete instruction; do not decompose. The circuit breaker will refuse calls past #5 in one session. Returns summary + metrics (turns_used, files_touched, verify_passed, verify_retries_used, input/output_tokens, elapsed_ms).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "instruction": {
                                    "type": "string",
                                    "description": "Natural-language task description. Be explicit about which files/directories are in scope and what success looks like."
                                },
                                "cwd": {
                                    "type": "string",
                                    "description": "Working directory for the inner agent's bash verify commands (optional, defaults to the MCP server's cwd)."
                                },
                                "verify": {
                                    "type": "string",
                                    "description": "Shell command to run for success verification, e.g. 'go test ./...' or 'pytest'. If set, the inner agent is required to run it and confirm green before returning."
                                },
                                "max_turns": {
                                    "type": "integer",
                                    "description": "Hard cap on inner agent turns (default 30)."
                                },
                                "model": {
                                    "type": "string",
                                    "description": "Override the inner model (default: nebula-host-30b on the configured Ollama endpoint)."
                                }
                            },
                            "required": ["instruction"]
                        }
                    }
                ]);

            // Tool-surface filter. When NEBULA_MCP_TOOLS is set to a
            // comma-separated allow-list (e.g. "code_task"), tools/list only
            // returns those. Lets benchmarks restrict the MCP server to a
            // single-tool delegate surface without needing Claude Code flags.
            let tools_value = match std::env::var("NEBULA_MCP_TOOLS") {
                Ok(spec) if !spec.trim().is_empty() => {
                    let allow: std::collections::HashSet<String> = spec
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let filtered: Vec<Value> = all_tools
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter(|t| {
                                    t.get("name")
                                        .and_then(|n| n.as_str())
                                        .map(|n| allow.contains(n))
                                        .unwrap_or(false)
                                })
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    json!(filtered)
                }
                _ => all_tools,
            };

            Some(jsonrpc_response(id, json!({ "tools": tools_value })))
        }

        "tools/call" => {
            let params = request.get("params")?;
            let tool_name = params.get("name")?.as_str()?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));

            let result = match tool_name {
                "read_code" => tool_read_code(&args),
                "write_code" => tool_write_code(&args),
                "patch_code" => tool_patch_code(&args),
                "search_code" => tool_search_code(&args),
                "translate_code" => tool_translate_code(&args),
                "code_task" => tool_code_task(&args),
                _ => Err(format!("Unknown tool: {tool_name}")),
            };

            Some(jsonrpc_response(id, match result {
                Ok(content) => json!({
                    "content": [{ "type": "text", "text": content }]
                }),
                Err(e) => json!({
                    "isError": true,
                    "content": [{ "type": "text", "text": e }]
                }),
            }))
        }

        _ => None,
    }
}

// ── Tool implementations ──

fn tool_read_code(args: &Value) -> Result<String, String> {
    let path = args.get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;

    let (raw, nebula) = read_source_cached(path)?;
    let lang = lang_detect::detect(path);

    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = args.get("limit").and_then(|v| v.as_u64());

    // Small files: return raw source verbatim. The MCP primer overhead exceeds
    // any compression saving on files this small, so just hand the model the
    // bytes it would otherwise need to cat anyway.
    let raw_line_count = raw.lines().count();
    // NEBULA_DISABLE_IL=1 forces every read to return raw source. Used by
    // the delegate-native MCP config in the iter-18/19 large-file ablation
    // to isolate the IL contribution from the tool-hiding contribution.
    let il_disabled = std::env::var("NEBULA_DISABLE_IL")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let bypass_compression = il_disabled
        || raw_line_count <= SMALL_FILE_THRESHOLD
        || lang == lang_detect::Language::Nebula
        || lang == lang_detect::Language::Unknown;

    let body = if bypass_compression { &raw } else { &nebula };
    let lines: Vec<&str> = body.lines().collect();
    let total = lines.len();

    let start = if offset > 0 { offset.saturating_sub(1) } else { 0 }; // 1-indexed
    let end = match limit {
        Some(l) => (start + l as usize).min(total),
        None => total,
    };

    let sliced: Vec<&str> = lines[start.min(total)..end.min(total)].to_vec();
    let numbered: Vec<String> = sliced.iter().enumerate()
        .map(|(i, line)| format!("{}\t{}", start + i + 1, line))
        .collect();
    let body_text = numbered.join("\n");

    if bypass_compression {
        // Raw mode: skip the Nebula primer header so the model knows this isn't compressed.
        if lang != lang_detect::Language::Nebula && lang != lang_detect::Language::Unknown {
            Ok(format!("// source: {} ({}, raw — file under {} lines)\n{}",
                       path, lang.name(), SMALL_FILE_THRESHOLD, body_text))
        } else {
            Ok(body_text)
        }
    } else {
        // Compressed mode: prepend a short native-source preview so the host model
        // can anchor on the literal package/imports/indentation style without
        // having to shell out to cat. The cost is a few dozen tokens; the saving
        // is the verification roundtrip we saw in earlier runs.
        let preview = raw.lines()
            .take(PREVIEW_LINES)
            .map(|l| format!("// {}", l))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(format!(
            "// source: {} ({}, Nebula form — preview of native first {} lines below)\n{}\n// === Nebula form ===\n{}",
            path, lang.name(), PREVIEW_LINES, preview, body_text
        ))
    }
}

fn tool_write_code(args: &Value) -> Result<String, String> {
    let path = args.get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;
    let code = args.get("code")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: code")?;

    let lang = lang_detect::detect(path);
    let lang_name = lang.name();

    // Try neural translation first (produces more idiomatic output)
    let output = if neural::is_available() {
        match neural::generate(code, lang_name, None) {
            Ok(ref neural_output) if neural::validate_output(neural_output, lang_name, code) => {
                neural_output.clone()
            }
            _ => lang_detect::from_nebula_lenient(code, lang)?,
        }
    } else {
        lang_detect::from_nebula_lenient(code, lang)?
    };

    fs::write(path, &output)
        .map_err(|e| format!("Failed to write '{}': {}", path, e))?;
    invalidate_cache(path);

    Ok(format!("Wrote {} bytes to {} ({})", output.len(), path, lang_name))
}

fn tool_patch_code(args: &Value) -> Result<String, String> {
    let path = args.get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;
    let old_str_arg = args.get("old_str")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: old_str")?;
    let new_str_arg = args.get("new_str")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: new_str")?;
    let replace_all = args.get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let glob = args.get("glob").and_then(|v| v.as_str());

    if old_str_arg.is_empty() {
        return Err("old_str must not be empty".to_string());
    }

    if let Some(glob_pattern) = glob {
        return tool_patch_code_bulk(path, glob_pattern, old_str_arg, new_str_arg);
    }

    let (source, _nebula) = read_source_cached(path)?;
    let lang = lang_detect::detect(path);
    let lang_name = lang.name();
    let (old_str, new_str) = (old_str_arg, new_str_arg);

    let occurrences = source.matches(old_str).count();
    if occurrences == 0 {
        return Err(format!(
            "patch_code: old_str not found in {}. Note: patch_code expects native-language source (e.g. Go, Python), not Nebula. Read the file with the standard tools or check whitespace/indentation.",
            path
        ));
    }
    if occurrences > 1 && !replace_all {
        return Err(format!(
            "patch_code: old_str matches {} locations in {}. Pass replace_all=true or extend old_str with more context to disambiguate.",
            occurrences, path
        ));
    }

    let output = if replace_all {
        source.replace(old_str, new_str)
    } else {
        source.replacen(old_str, new_str, 1)
    };

    fs::write(path, &output)
        .map_err(|e| format!("Failed to write '{}': {}", path, e))?;
    invalidate_cache(path);

    Ok(format!(
        "Patched {} in {} ({} {} replaced, {} bytes)",
        path,
        lang_name,
        occurrences,
        if occurrences == 1 { "occurrence" } else { "occurrences" },
        output.len()
    ))
}

/// Bulk mode: walk `dir_path`, apply the replacement to every file matching
/// `glob_pattern` that contains `old_str`. Implicit replace_all. Returns a
/// per-file summary plus totals. One tool call covers the whole rename.
fn tool_patch_code_bulk(
    dir_path: &str,
    glob_pattern: &str,
    old_str: &str,
    new_str: &str,
) -> Result<String, String> {
    let dir = Path::new(dir_path);
    if !dir.is_dir() {
        return Err(format!(
            "patch_code (bulk mode): path '{}' is not a directory. When `glob` is set, `path` must be a directory to walk.",
            dir_path
        ));
    }

    let mut changed: Vec<(String, usize)> = Vec::new();
    let mut skipped_binary: usize = 0;
    let mut scanned: usize = 0;
    let mut write_errors: Vec<(String, String)> = Vec::new();

    walk_dir(dir, &mut |file_path| {
        let file_name = match file_path.file_name() {
            Some(n) => n.to_string_lossy().into_owned(),
            None => return,
        };
        if !glob_match(glob_pattern, &file_name) {
            return;
        }
        scanned += 1;

        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => {
                skipped_binary += 1;
                return;
            }
        };
        if source.bytes().take(512).any(|b| b == 0) {
            skipped_binary += 1;
            return;
        }

        let occurrences = source.matches(old_str).count();
        if occurrences == 0 {
            return;
        }

        let output = source.replace(old_str, new_str);
        let path_str = file_path.to_string_lossy().into_owned();
        match fs::write(file_path, &output) {
            Ok(()) => {
                invalidate_cache(&path_str);
                changed.push((path_str, occurrences));
            }
            Err(e) => {
                write_errors.push((path_str, e.to_string()));
            }
        }
    });

    if changed.is_empty() && write_errors.is_empty() {
        return Err(format!(
            "patch_code (bulk mode): no files under '{}' matching '{}' contained old_str. Scanned {} file(s). Note: patch_code expects native-language source (Go, Python, etc.), not Nebula.",
            dir_path, glob_pattern, scanned
        ));
    }

    let mut report = String::new();
    let total_occurrences: usize = changed.iter().map(|(_, n)| n).sum();
    report.push_str(&format!(
        "Bulk patch: {} file(s) modified, {} total replacement(s) under '{}' matching '{}'.\n",
        changed.len(), total_occurrences, dir_path, glob_pattern
    ));
    if skipped_binary > 0 {
        report.push_str(&format!("Skipped {} binary/unreadable file(s).\n", skipped_binary));
    }
    for (p, n) in &changed {
        report.push_str(&format!("  {} ({} replaced)\n", p, n));
    }
    if !write_errors.is_empty() {
        report.push_str(&format!("WRITE ERRORS ({}):\n", write_errors.len()));
        for (p, e) in &write_errors {
            report.push_str(&format!("  {}: {}\n", p, e));
        }
    }
    Ok(report.trim_end().to_string())
}

fn tool_search_code(args: &Value) -> Result<String, String> {
    let query = args.get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;

    let search_path = args.get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let glob_pattern = args.get("glob")
        .and_then(|v| v.as_str());

    let query_lower = query.to_lowercase();
    let mut results: Vec<Value> = Vec::new();
    let max_results = 50;

    walk_dir(Path::new(search_path), &mut |file_path| {
        if results.len() >= max_results {
            return;
        }

        let path_str = file_path.to_string_lossy().to_string();

        // Apply glob filter if specified
        if let Some(pattern) = glob_pattern {
            let file_name = file_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !glob_match(pattern, &file_name) {
                return;
            }
        }

        let lang = lang_detect::detect(&path_str);

        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => return,
        };

        // Skip binary files
        if source.bytes().take(512).any(|b| b == 0) {
            return;
        }

        let lines: Vec<&str> = source.lines().collect();

        // ── Pass 1: raw source substring match (literal target-language hits) ──
        let mut raw_match_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for (i, line) in lines.iter().enumerate() {
            if results.len() >= max_results { break; }
            if line.to_lowercase().contains(&query_lower) {
                let start = i.saturating_sub(3);
                let end = (i + 4).min(lines.len());
                let snippet: String = lines[start..end].join("\n");
                raw_match_lines.insert(i + 1);
                results.push(json!({
                    "path": path_str,
                    "line": i + 1,
                    "match_form": "raw",
                    "language": lang.name(),
                    "snippet": snippet
                }));
            }
        }

        // ── Pass 2: Nebula canonical form substring match (cross-language hits) ──
        // Catches matches that survived snake_case / method-alias / type-name normalization
        // but not the literal source. E.g., querying "reverse(" finds Go's `Reverse(` too.
        if results.len() < max_results
            && !matches!(lang, lang_detect::Language::Nebula | lang_detect::Language::Unknown)
        {
            let nebula_canonical = lang_detect::to_nebula_with_opts(&source, lang, false);
            let nebula_lines: Vec<&str> = nebula_canonical.lines().collect();
            for (i, line) in nebula_lines.iter().enumerate() {
                if results.len() >= max_results { break; }
                if line.to_lowercase().contains(&query_lower) {
                    // Approximate source line (rough heuristic — Nebula lines roughly
                    // correspond to source lines minus brace-only lines collapsed by
                    // braces_to_indent). Clamp to file length.
                    let approx_source_line = (i + 1).min(lines.len()).max(1);
                    // Skip if a raw match already covered roughly this region
                    if raw_match_lines.iter().any(|&l| l.abs_diff(approx_source_line) <= 2) {
                        continue;
                    }
                    let start = i.saturating_sub(2);
                    let end = (i + 3).min(nebula_lines.len());
                    let snippet: String = nebula_lines[start..end].join("\n");
                    results.push(json!({
                        "path": path_str,
                        "approx_line": approx_source_line,
                        "match_form": "nebula",
                        "language": lang.name(),
                        "snippet": snippet,
                        "note": "matched canonical Nebula form — read_code the file for exact source line"
                    }));
                }
            }
        }
    });

    if results.is_empty() {
        Ok(format!("No matches found for '{}'", query))
    } else {
        Ok(serde_json::to_string_pretty(&results).unwrap_or_default())
    }
}

fn tool_translate_code(args: &Value) -> Result<String, String> {
    let code = args.get("code")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: code")?;
    let source_language = args.get("source_language")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: source_language")?;
    let target_language = args.get("target_language")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: target_language")?;

    // Ingest source code to Nebula
    let src_lang = lang_detect::detect(&format!("file.{}", lang_ext(source_language)));
    let nebula = lang_detect::to_nebula(code, src_lang);

    // Try neural translation first
    if neural::is_available() {
        if let Ok(ref output) = neural::generate(&nebula, target_language, None) {
            if neural::validate_output(output, target_language, &nebula) {
                return Ok(output.clone());
            }
        }
    }

    // Fall back to lenient rule-based codegen
    let tgt_lang = lang_detect::detect(&format!("file.{}", lang_ext(target_language)));
    lang_detect::from_nebula_lenient(&nebula, tgt_lang)
}

// ── Translation fast-path ──

const LANG_EXTENSIONS: &[(&str, &str)] = &[
    ("python", "py"), ("go", "go"), ("rust", "rs"), ("javascript", "js"),
    ("typescript", "ts"), ("swift", "swift"), ("kotlin", "kt"), ("c", "c"),
];

/// Check if a language name appears at a word boundary in text.
fn has_lang_word(text: &str, lang: &str) -> bool {
    let bytes = text.as_bytes();
    let pat = lang.as_bytes();
    let mut start = 0;
    while let Some(pos) = text[start..].find(lang) {
        let abs = start + pos;
        let before_ok = abs == 0 || !bytes[abs - 1].is_ascii_alphanumeric();
        let after_ok = abs + pat.len() >= bytes.len()
            || !bytes[abs + pat.len()].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

/// Detect translation-shaped tasks. Returns (source_lang, target_lang) if the
/// instruction matches "port/translate/convert ... to <language>".
fn detect_translation(instruction: &str) -> Option<(String, String)> {
    let lower = instruction.to_lowercase();
    let has_verb = ["port ", "translate ", "convert ", "rewrite "]
        .iter().any(|v| lower.contains(v));
    if !has_verb { return None; }

    // Extract target language after "to <lang>" or "into <lang>"
    let target = LANG_EXTENSIONS.iter()
        .find(|(name, _)| {
            lower.contains(&format!(" to {} ", name))
                || lower.contains(&format!(" to {}\n", name))
                || lower.ends_with(&format!(" to {}", name))
                || lower.contains(&format!(" into {} ", name))
                || lower.contains(&format!(" into {}\n", name))
        })
        .map(|(name, _)| name.to_string())?;

    // Detect source language via file extensions (.py, .go, .rs) or word-boundary
    // name matches, excluding the target language.
    let source = LANG_EXTENSIONS.iter()
        .find(|(name, ext)| {
            *name != target.as_str() && (
                lower.contains(&format!(".{} ", ext))
                || lower.contains(&format!(".{}\n", ext))
                || lower.ends_with(&format!(".{}", ext))
                || lower.contains(&format!(".{}`", ext))
                || (name.len() >= 3 && has_lang_word(&lower, name))
            )
        })
        .map(|(name, _)| name.to_string())?;

    Some((source, target))
}

/// Find the source file in cwd matching the source language.
fn find_source_file(cwd: &str, src_lang: &str) -> Option<(String, String)> {
    let ext = LANG_EXTENSIONS.iter()
        .find(|(n, _)| *n == src_lang)
        .map(|(_, e)| *e)?;
    let dir = std::fs::read_dir(cwd).ok()?;
    for entry in dir.flatten() {
        let path = entry.path();
        if let Some(e) = path.extension() {
            if e == ext {
                let name = path.file_name()?.to_string_lossy().into_owned();
                // Skip test files
                if name.contains("_test.") || name.contains("test_") || name.ends_with("_test.go") {
                    continue;
                }
                let full = path.to_string_lossy().into_owned();
                let content = std::fs::read_to_string(&full).ok()?;
                return Some((full, content));
            }
        }
    }
    None
}

/// Find the target file in cwd matching the target language.
fn find_target_file(cwd: &str, tgt_lang: &str) -> Option<String> {
    let ext = LANG_EXTENSIONS.iter()
        .find(|(n, _)| *n == tgt_lang)
        .map(|(_, e)| *e)?;
    let dir = std::fs::read_dir(cwd).ok()?;
    for entry in dir.flatten() {
        let path = entry.path();
        if let Some(e) = path.extension() {
            if e == ext {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if name.contains("_test.") || name.contains("test_") {
                    continue;
                }
                return Some(path.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// Extract code from a markdown fenced block if present, otherwise return as-is.
fn extract_code_block(text: &str) -> &str {
    if let Some(start) = text.find("```") {
        let after_fence = &text[start + 3..];
        // Skip the language tag on the opening fence
        let code_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
        let code = &after_fence[code_start..];
        if let Some(end) = code.find("```") {
            return code[..end].trim();
        }
    }
    text.trim()
}

/// Try the compiler translation fast-path. Returns Some(Ok/Err) if the fast-path
/// handled the task (success or hard error), None to fall through to the agent loop.
fn try_translation_fast_path(
    instruction: &str,
    cwd: &str,
    verify: Option<&str>,
    preamble: &str,
) -> Option<Result<String, String>> {
    let (src_lang, tgt_lang) = detect_translation(instruction)?;

    let (source_path, source_code) = find_source_file(cwd, &src_lang)?;
    let target_path = find_target_file(cwd, &tgt_lang)?;

    eprintln!("[translate-fast] detected: {} → {}", src_lang, tgt_lang);
    eprintln!("[translate-fast] source: {}, target: {}", source_path, target_path);

    let original_target = std::fs::read_to_string(&target_path).unwrap_or_default();
    let t0 = std::time::Instant::now();

    // Direct translation prompt — give the model the source code and target API
    // signatures, let it produce idiomatic target code in one shot.
    let translate_prompt = format!(
        "Translate this {} code to {}.\n\n\
         {} source:\n```{}\n{}\n```\n\n\
         {} target file with API signatures to match:\n```{}\n{}\n```\n\n\
         Write a complete {} implementation that preserves the exact same \
         semantics and matches the API signatures above. Output ONLY the {} code.",
        src_lang, tgt_lang,
        src_lang, src_lang, source_code,
        tgt_lang, tgt_lang, original_target,
        tgt_lang, tgt_lang,
    );

    eprintln!("[translate-fast] sending direct translation prompt to model");

    let max_passes = 2;
    let mut current_prompt = translate_prompt;

    for pass in 1..=max_passes {
        match mcp_delegate::run_single_correction(&current_prompt) {
            Ok(response) => {
                let code = extract_code_block(&response);
                if std::fs::write(&target_path, code).is_err() {
                    break;
                }

                if let Some(cmd) = verify {
                    let output = std::process::Command::new("sh")
                        .arg("-c").arg(cmd).current_dir(cwd).output();
                    if let Ok(out) = &output {
                        if out.status.success() {
                            eprintln!("[translate-fast] pass {} passed verify!", pass);
                            let result = serde_json::json!({
                                "summary": format!(
                                    "Translation fast-path: direct {} → {} translation \
                                     succeeded in {} pass(es). All tests pass.{}",
                                    src_lang, tgt_lang, pass, preamble
                                ),
                                "turns_used": pass,
                                "tool_calls": 0,
                                "elapsed_ms": t0.elapsed().as_millis() as u64,
                                "stopped_reason": format!("translate_fast_path_pass_{}", pass),
                                "verify_passed": true,
                                "files_touched": [&target_path],
                            });
                            return Some(Ok(serde_json::to_string_pretty(&result).unwrap()));
                        }
                        // Build next prompt from current code + errors
                        if pass < max_passes {
                            let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
                            combined.push_str(&String::from_utf8_lossy(&out.stderr));
                            if combined.len() > 3000 { combined.truncate(3000); }
                            eprintln!("[translate-fast] pass {} failed, retrying with errors", pass);
                            current_prompt = format!(
                                "Fix this {} code so all tests pass.\n\n\
                                 Current code:\n```{}\n{}\n```\n\n\
                                 Test errors:\n```\n{}\n```\n\n\
                                 Output ONLY the corrected file.",
                                tgt_lang, tgt_lang, code, combined
                            );
                        }
                    }
                } else {
                    // No verify command — can't validate, just return
                    let result = serde_json::json!({
                        "summary": format!(
                            "Translation fast-path: direct {} → {} translation. \
                             No verify command provided.{}",
                            src_lang, tgt_lang, preamble
                        ),
                        "turns_used": pass,
                        "tool_calls": 0,
                        "elapsed_ms": t0.elapsed().as_millis() as u64,
                        "stopped_reason": "translate_fast_path_no_verify",
                        "verify_passed": serde_json::Value::Null,
                        "files_touched": [&target_path],
                    });
                    return Some(Ok(serde_json::to_string_pretty(&result).unwrap()));
                }
            }
            Err(e) => {
                eprintln!("[translate-fast] pass {} failed: {}", pass, e);
                break;
            }
        }
    }

    eprintln!("[translate-fast] all passes exhausted, falling through to agent");
    let _ = std::fs::write(&target_path, &original_target);
    None
}

/// `code_task` — delegate a natural-language coding task to the inner
/// Nebula-native agent (default: nebula-host-30b on Ollama). The inner agent
/// drives the existing read_code/write_code/patch_code/search_code tools
/// directly in-process (no recursion, no JSON-RPC round-trips) and returns a
/// short English summary plus metrics. See `compiler/src/mcp_delegate.rs` for
/// the agent loop and `bench/loop_notes.md` iteration 1 for the rationale.
fn tool_code_task(args: &Value) -> Result<String, String> {
    let instruction = args
        .get("instruction")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: instruction")?;

    if !mcp_delegate::is_available() {
        return Err(format!(
            "code_task: inner Ollama endpoint not reachable at {}. \
             Delegate coder is unavailable — fall back to calling read_code / \
             patch_code / write_code directly, or set NEBULA_DELEGATE_OLLAMA_URL \
             to a running Ollama instance.",
            mcp_delegate::default_delegate_url()
        ));
    }

    // Session-level call-count circuit breaker. Iteration 5 saw an implement-
    // task run where Sonnet made 17 `code_task` calls in a repair death
    // spiral (+242% cost vs builtins). If the host calls us 5+ times in a
    // single MCP server lifetime, that's a strong signal the task shape
    // isn't a delegate fit — refuse further calls with a structured hint.
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
    let this_call = CALL_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    let mut preamble = String::new();
    if this_call >= 5 {
        return Err(format!(
            "code_task: refusing call #{} in this MCP session. The delegate \
             tool is designed for one-call-handles-the-whole-task workflows; \
             calling it 5+ times usually means the task shape doesn't fit \
             delegation, or the inner agent is drifting. Switch to the \
             low-level tools (read_code, patch_code, write_code) for the \
             remaining work, or restart the MCP server if you really need \
             the delegate again. Override by unsetting NEBULA_MCP_TOOLS to \
             expose the low-level tools too.",
            this_call
        ));
    }
    if this_call >= 3 {
        preamble.push_str(&format!(
            "\n\n[delegate hint] This is code_task call #{} in this session. \
             Consider batching remaining work into ONE more call with a \
             complete instruction + verify command, or switch to low-level \
             tools. Further calls past #5 will be refused.",
            this_call
        ));
    }

    let cwd_arg = args.get("cwd").and_then(|v| v.as_str());
    let cwd_owned = cwd_arg
        .map(|s| s.to_string())
        .or_else(|| std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| ".".to_string());

    let verify = args.get("verify").and_then(|v| v.as_str());

    // Preflight: if a verify command is set, run it once BEFORE handing the
    // instruction to the inner agent. Prepend the current failure output to
    // the instruction so the inner agent doesn't burn a turn discovering what
    // is broken. On bugfix-shaped tasks this skips the first "run tests to
    // see state" round. On refactor-shaped tasks the preflight passes and
    // this costs ~50ms for nothing — acceptable.
    let preflight_note: String = if let Some(cmd) = verify {
        let preflight_output = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&cwd_owned)
            .output();
        match preflight_output {
            Ok(out) if !out.status.success() => {
                let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
                combined.push_str(&String::from_utf8_lossy(&out.stderr));
                let tail_len = combined.len().min(2000);
                let preview = if combined.len() > 2000 {
                    &combined[combined.len() - 2000..]
                } else {
                    &combined[..]
                };
                let _ = tail_len;
                format!(
                    "\n\n== Current verify state ==\n\
                     Before you start, here is the output of `{}` on the \
                     current files. Use this to identify exactly what needs \
                     to change. Fix the failures then re-run the command to \
                     confirm:\n\n```\nexit {}\n{}\n```",
                    cmd,
                    out.status.code().unwrap_or(-1),
                    preview
                )
            }
            _ => String::new(),
        }
    } else {
        String::new()
    };
    let model_override = args.get("model").and_then(|v| v.as_str());

    // ── Translation fast-path ──
    // If the instruction is a pure translation/porting task, try the compiler
    // pipeline (instant, free) before spinning up the agent loop. Compiler
    // translates source → Nebula → target; if verify passes, return immediately.
    // If verify fails, do a single-pass Ollama correction. Falls through to the
    // normal agent loop only if both fail.
    if let Some(fast_result) = try_translation_fast_path(
        instruction, &cwd_owned, verify, &preamble,
    ) {
        return fast_result;
    }

    let mut config = mcp_delegate::DelegateConfig::with_defaults();
    // Respect a caller-supplied max_turns only if larger than the default —
    // iteration 7 found Sonnet sometimes passed small values (10, 40) that
    // prevented the inner agent from converging on multi-file refactors.
    if let Some(n) = args.get("max_turns").and_then(|v| v.as_u64()) {
        config.max_turns = (n as usize).max(config.max_turns);
    }
    if let Some(m) = model_override {
        config.model = m.to_string();
    }

    // Tool schemas the inner agent sees: the existing MCP tools, minus
    // `code_task` itself (no recursion) and minus `translate_code` (not useful
    // inside a delegate session — it handles file I/O, not standalone strings).
    let inner_tools: Vec<Value> = vec![
        inner_tool_schema("read_code"),
        inner_tool_schema("write_code"),
        inner_tool_schema("patch_code"),
        inner_tool_schema("search_code"),
    ];

    // Rewrite relative path args against the delegate's cwd. The inner agent
    // naturally uses bare filenames like "strings_util.go"; without this, the
    // underlying handlers would resolve against the MCP server's process cwd
    // and corrupt whatever directory nebula was launched from.
    let cwd_for_dispatch = cwd_owned.clone();
    let resolve_paths = move |args: &Value| -> Value {
        let Some(obj) = args.as_object() else { return args.clone(); };
        let mut out = obj.clone();
        for key in ["path", "file", "filename"] {
            if let Some(Value::String(s)) = out.get(key).cloned() {
                let p = std::path::Path::new(&s);
                if p.is_relative() {
                    let joined = std::path::Path::new(&cwd_for_dispatch).join(p);
                    out.insert(key.to_string(), Value::String(joined.to_string_lossy().into_owned()));
                }
            }
        }
        Value::Object(out)
    };

    let dispatch = move |name: &str, tool_args: &Value| -> Result<String, String> {
        let resolved = resolve_paths(tool_args);
        match name {
            "read_code" => tool_read_code(&resolved),
            "write_code" => tool_write_code(&resolved),
            "patch_code" => tool_patch_code(&resolved),
            "search_code" => tool_search_code(&resolved),
            "translate_code" => tool_translate_code(&resolved),
            other => Err(format!("Unknown tool in delegate loop: {}", other)),
        }
    };

    let full_instruction: String = if preflight_note.is_empty() {
        instruction.to_string()
    } else {
        format!("{}{}", instruction, preflight_note)
    };

    let result = mcp_delegate::run_delegate(
        &config,
        &full_instruction,
        &inner_tools,
        Some(&cwd_owned),
        verify,
        dispatch,
    )?;

    // run_delegate already ran verify in-loop (with up to max_verify_retries
    // repair rounds) and populated result.verify_passed / verify_output_tail.
    let summary_text = if preamble.is_empty() {
        result.final_message.clone()
    } else {
        format!("{}{}", result.final_message, preamble)
    };

    let summary = serde_json::json!({
        "summary": summary_text,
        "turns_used": result.turns_used,
        "tool_calls": result.tool_calls,
        "tool_calls_by_name": result.tool_calls_by_name,
        "files_touched": result.files_touched,
        "input_tokens": result.input_tokens,
        "output_tokens": result.output_tokens,
        "elapsed_ms": result.elapsed_ms,
        // Phase 20 instrumentation: inner-loop wall-clock breakdown so external
        // runners can answer "is Ollama inference the bottleneck, or is it tool
        // dispatch / bash / verify?" without re-plumbing the delegate.
        "total_prefill_ms": result.total_prefill_ms,
        "total_gen_ms": result.total_gen_ms,
        "total_ollama_wall_ms": result.total_ollama_wall_ms,
        "total_tool_dispatch_ms": result.total_tool_dispatch_ms,
        "total_bash_ms": result.total_bash_ms,
        "per_turn": result.per_turn,
        "stopped_reason": result.stopped_reason,
        "verify_passed": result.verify_passed,
        "verify_retries_used": result.verify_retries_used,
        "verify_output_tail": result.verify_output_tail,
        "warnings": result.warnings,
        "session_call_number": this_call,
        "model": config.model,
    });

    Ok(serde_json::to_string_pretty(&summary).unwrap_or_else(|_| result.final_message.clone()))
}

/// Build an inner-agent tool schema by name. We can't borrow the tools/list
/// output at this call site, so we reconstruct just the shapes the inner agent
/// needs. These mirror the tools/list entries above — keep them in sync.
fn inner_tool_schema(name: &str) -> Value {
    match name {
        "read_code" => json!({
            "name": "read_code",
            "description": "Read a source file translated to Nebula for token efficiency. Returns code in Nebula's compressed syntax.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "offset": {"type": "integer"},
                    "limit": {"type": "integer"}
                },
                "required": ["path"]
            }
        }),
        "write_code" => json!({
            "name": "write_code",
            "description": "Write Nebula code to a file, auto-translating to the file's native language based on extension.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "code": {"type": "string"}
                },
                "required": ["path", "code"]
            }
        }),
        "patch_code" => json!({
            "name": "patch_code",
            "description": "Edit source by replacing a snippet. old_str / new_str are NATIVE language (not Nebula). Pass a directory `path` plus `glob` for bulk rename (single call). Use replace_all=true for repetitive substitutions in one file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "old_str": {"type": "string"},
                    "new_str": {"type": "string"},
                    "replace_all": {"type": "boolean"},
                    "glob": {"type": "string"}
                },
                "required": ["path", "old_str", "new_str"]
            }
        }),
        "search_code" => json!({
            "name": "search_code",
            "description": "Search codebase for a pattern, returning matching snippets translated to Nebula.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "path": {"type": "string"},
                    "glob": {"type": "string"}
                },
                "required": ["query"]
            }
        }),
        _ => json!({}),
    }
}

/// Map language name to a file extension for lang_detect::detect().
fn lang_ext(lang: &str) -> &str {
    match lang.to_lowercase().as_str() {
        "python" | "py" => "py",
        "javascript" | "js" => "js",
        "typescript" | "ts" => "ts",
        "go" => "go",
        "swift" => "swift",
        "kotlin" | "kt" => "kt",
        "rust" | "rs" => "rs",
        "c" => "c",
        "nebula" | "neb" => "neb",
        _ => lang,
    }
}

// ── Helpers ──

fn jsonrpc_response(id: Option<&Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.cloned().unwrap_or(Value::Null),
        "result": result
    })
}

/// Simple directory walker that skips common noise directories.
fn walk_dir(dir: &Path, callback: &mut dyn FnMut(&Path)) {
    let skip_dirs = [".git", "node_modules", "target", "__pycache__", ".venv",
                     "venv", ".tox", "dist", "build", ".next", ".nuxt"];

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Skip hidden files/dirs (except specific ones) and noise dirs
            if name.starts_with('.') || skip_dirs.contains(&name) {
                continue;
            }
        }

        if path.is_dir() {
            walk_dir(&path, callback);
        } else if path.is_file() {
            callback(&path);
        }
    }
}

/// Simple glob matching: supports * and ? patterns.
fn glob_match(pattern: &str, name: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = name.chars().collect();
    glob_match_inner(&pat, &txt)
}

fn glob_match_inner(pat: &[char], txt: &[char]) -> bool {
    match (pat.first(), txt.first()) {
        (None, None) => true,
        (Some('*'), _) => {
            // * matches zero or more characters
            glob_match_inner(&pat[1..], txt)
                || (!txt.is_empty() && glob_match_inner(pat, &txt[1..]))
        }
        (Some('?'), Some(_)) => glob_match_inner(&pat[1..], &txt[1..]),
        (Some(a), Some(b)) if a == b => glob_match_inner(&pat[1..], &txt[1..]),
        _ => false,
    }
}
