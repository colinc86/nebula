/// MCP (Model Context Protocol) server for Nebula.
///
/// Implements JSON-RPC 2.0 over stdio with three tools:
/// - read_code: Read source files translated to Nebula
/// - write_code: Write Nebula code translated to target language
/// - search_code: Search codebase with results in Nebula

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::fs;
use std::path::Path;

use crate::lang_detect;
use crate::neural;

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
                }
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

    let source = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read '{}': {}", path, e))?;

    let lang = lang_detect::detect(path);
    let nebula = lang_detect::to_nebula(&source, lang);

    // Apply offset/limit if specified
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = args.get("limit").and_then(|v| v.as_u64());

    let lines: Vec<&str> = nebula.lines().collect();
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

    let mut result = numbered.join("\n");
    if lang != lang_detect::Language::Nebula && lang != lang_detect::Language::Unknown {
        result = format!("// source: {} ({})\n{}", path, lang.name(), result);
    }

    Ok(result)
}

fn tool_write_code(args: &Value) -> Result<String, String> {
    let path = args.get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;
    let code = args.get("code")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: code")?;

    let lang = lang_detect::detect(path);
    let output = lang_detect::from_nebula(code, lang)?;

    fs::write(path, &output)
        .map_err(|e| format!("Failed to write '{}': {}", path, e))?;

    Ok(format!("Wrote {} bytes to {} ({})", output.len(), path, lang.name()))
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

        let path_str = file_path.to_string_lossy();

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

        // Check for binary content
        if source.bytes().take(512).any(|b| b == 0) {
            return;
        }

        let lines: Vec<&str> = source.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if results.len() >= max_results {
                break;
            }

            if line.to_lowercase().contains(&query_lower) {
                // Extract context: 3 lines before and after
                let start = i.saturating_sub(3);
                let end = (i + 4).min(lines.len());
                let snippet: String = lines[start..end].join("\n");

                let nebula_snippet = lang_detect::to_nebula(&snippet, lang);

                results.push(json!({
                    "path": path_str,
                    "line": i + 1,
                    "language": lang.name(),
                    "snippet": nebula_snippet
                }));
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
        if let Ok(output) = neural::generate(&nebula, target_language, None) {
            return Ok(output);
        }
    }

    // Fall back to rule-based codegen
    let tgt_lang = lang_detect::detect(&format!("file.{}", lang_ext(target_language)));
    lang_detect::from_nebula(&nebula, tgt_lang)
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
