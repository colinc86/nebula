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
Workflow: read_code to view, patch_code to edit, write_code only for new files \
or full rewrites. patch_code is byte-level on the raw source bytes, so old_str / \
new_str must be in the file's NATIVE language (Go, Python, etc.) — NOT Nebula. \
Use replace_all=true for repetitive substitutions within one file.\n\
\n\
For package-wide renames across many files, use patch_code BULK mode in a \
SINGLE call: pass `path` as the directory and `glob` like '*.go'. It walks \
the directory and applies the replacement to every matching file — one tool \
call, not one per file. Prefer this over `bash sed -i` loops: same semantics, \
lower tool-call overhead, and you get a structured per-file report back.\n\
\n\
Nebula syntax: `fn name(a: Int) -> Int:` indent body / `rt x` / `fo i in 0..n:` / \
`x := 5` immutable, `x = 5` mutable / `\"hi {name}\"` interpolation / `|x| x+1` \
lambdas / `xs.map { it.v }` trailing closures / `and or not ??  |>` / `True False None`. \
Both short (`fn rt fo`) and long (`def return for`) keyword forms are 1 token.\
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
            Some(jsonrpc_response(id, json!({
                "tools": [
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
                    }
                ]
            })))
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
    let bypass_compression = raw_line_count <= SMALL_FILE_THRESHOLD
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
    let old_str = args.get("old_str")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: old_str")?;
    let new_str = args.get("new_str")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: new_str")?;
    let replace_all = args.get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let glob = args.get("glob").and_then(|v| v.as_str());

    if old_str.is_empty() {
        return Err("old_str must not be empty".to_string());
    }

    if let Some(glob_pattern) = glob {
        return tool_patch_code_bulk(path, glob_pattern, old_str, new_str);
    }

    let (source, _nebula) = read_source_cached(path)?;
    let lang = lang_detect::detect(path);
    let lang_name = lang.name();

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
