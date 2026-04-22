/// Language detection and dispatch for the MCP server.
///
/// Maps file extensions to languages and dispatches to the appropriate
/// ingest (source → Nebula) and codegen (Nebula → source) functions.

use crate::{ast, codegen, compactor, ingest, lexer, parser};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Language {
    Python,
    JavaScript,
    TypeScript,
    Go,
    Swift,
    Kotlin,
    Rust,
    C,
    Nebula,
    Unknown,
}

impl Language {
    pub fn name(&self) -> &'static str {
        match self {
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Go => "go",
            Language::Swift => "swift",
            Language::Kotlin => "kotlin",
            Language::Rust => "rust",
            Language::C => "c",
            Language::Nebula => "nebula",
            Language::Unknown => "unknown",
        }
    }
}

/// Detect language from file extension.
pub fn detect(path: &str) -> Language {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "py" => Language::Python,
        "js" | "mjs" | "cjs" => Language::JavaScript,
        "ts" | "tsx" | "mts" => Language::TypeScript,
        "go" => Language::Go,
        "swift" => Language::Swift,
        "kt" | "kts" => Language::Kotlin,
        "rs" => Language::Rust,
        "c" | "h" => Language::C,
        "neb" => Language::Nebula,
        _ => Language::Unknown,
    }
}

/// Convert source code to Nebula using the appropriate ingester.
pub fn to_nebula(source: &str, lang: Language) -> String {
    to_nebula_with_opts(source, lang, false)
}

/// Convert source to Nebula with optional comment preservation.
/// `keep_comments=true` retains `//` comment lines so the host model can read docstrings;
/// the canonical compression form (used by the benchmark) keeps it `false`.
pub fn to_nebula_with_opts(source: &str, lang: Language, keep_comments: bool) -> String {
    match lang {
        Language::Python => ingest::from_python_with_opts(source, keep_comments),
        Language::JavaScript => ingest::from_javascript_with_opts(source, keep_comments),
        Language::TypeScript => ingest::from_typescript_with_opts(source, keep_comments),
        Language::Go => ingest::from_go_with_opts(source, keep_comments),
        Language::Swift => ingest::from_swift_with_opts(source, keep_comments),
        Language::Kotlin => ingest::from_kotlin_with_opts(source, keep_comments),
        Language::Rust => ingest::from_rust_with_opts(source, keep_comments),
        Language::C => ingest::from_c_with_opts(source, keep_comments),
        Language::Nebula | Language::Unknown => source.to_string(),
    }
}

/// Convert Nebula code to target language using codegen.
pub fn from_nebula(nebula: &str, lang: Language) -> Result<String, String> {
    if lang == Language::Nebula || lang == Language::Unknown {
        return Ok(nebula.to_string());
    }

    // Expand compacted identifiers if present (@map{} / @std{} headers)
    let expanded = if nebula.starts_with("@map{") || nebula.starts_with("@std{") {
        compactor::expand(nebula)
    } else {
        nebula.to_string()
    };

    // Ingesters emit indent-block Nebula; convert to brace blocks for the parser.
    let braced = indent_to_braces(&expanded);

    let tokens = lexer::tokenize(&braced)
        .map_err(|e| format!("Lex error at {}:{}: {}", e.line, e.col, e.message))?;
    let program = parser::parse(tokens)
        .map_err(|e| format!("Parse error at {}:{}: {}", e.line, e.col, e.message))?;

    Ok(gen_target(&program, lang))
}

/// Path-aware Nebula → native translation.
///
/// Iter-24 Bug A fix: for Go output, derives the target package name from
/// `target_path`'s directory (by scanning sibling `*.go` files for their
/// `package X` declaration). All other languages fall through to the normal
/// `from_nebula` pipeline.
///
/// This compensates for the Go ingester discarding `package_clause` as a
/// Nebula comment, which means round-tripping `package gdsp` → Nebula →
/// Go previously always yielded `package main`.
pub fn from_nebula_to_path(
    nebula: &str,
    lang: Language,
    target_path: &str,
) -> Result<String, String> {
    if lang != Language::Go {
        return from_nebula(nebula, lang);
    }

    // Expand + parse (same as from_nebula).
    if lang == Language::Nebula || lang == Language::Unknown {
        return Ok(nebula.to_string());
    }
    let expanded = if nebula.starts_with("@map{") || nebula.starts_with("@std{") {
        compactor::expand(nebula)
    } else {
        nebula.to_string()
    };
    let braced = indent_to_braces(&expanded);
    let tokens = lexer::tokenize(&braced)
        .map_err(|e| format!("Lex error at {}:{}: {}", e.line, e.col, e.message))?;
    let program = parser::parse(tokens)
        .map_err(|e| format!("Parse error at {}:{}: {}", e.line, e.col, e.message))?;

    let pkg = derive_go_package(target_path);
    Ok(codegen::to_go_with_package(&program, pkg.as_deref()))
}

/// Derive a Go `package` name from the directory of `target_path`.
///
/// Strategy:
/// 1. Scan sibling `*.go` files (excluding `*_test.go` and `target_path`
///    itself) for their `package X` declaration.
/// 2. If all agree on a name, return it.
/// 3. If they disagree, emit a stderr warning and use the first.
/// 4. If no siblings exist, fall back to a sanitized form of the directory
///    name (lowercase ASCII-alnum; reserved words get a trailing underscore).
/// 5. If `target_path` is empty or lacks a parent, return `None` so the
///    caller defaults to `"main"`.
fn derive_go_package(target_path: &str) -> Option<String> {
    use std::fs;
    use std::path::Path;

    if target_path.is_empty() || target_path == "-" {
        return None;
    }
    let path = Path::new(target_path);
    let parent = path.parent()?;

    // Scan siblings for `package X` in their first non-comment, non-blank line.
    let mut seen: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p == path {
                continue;
            }
            let fname = match p.file_name().and_then(|s| s.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if !fname.ends_with(".go") || fname.ends_with("_test.go") {
                continue;
            }
            if let Ok(contents) = fs::read_to_string(&p) {
                for line in contents.lines() {
                    let t = line.trim();
                    if t.is_empty() || t.starts_with("//") || t.starts_with("/*") {
                        continue;
                    }
                    if let Some(rest) = t.strip_prefix("package ") {
                        let name = rest.trim().trim_end_matches(';').to_string();
                        if !name.is_empty() && !seen.contains(&name) {
                            seen.push(name);
                        }
                        break;
                    }
                    break; // first meaningful line wasn't a package clause; skip file
                }
            }
        }
    }

    match seen.len() {
        0 => {
            // Dirname fallback.
            let dir_name = parent
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            Some(sanitize_go_package_name(&dir_name))
        }
        1 => Some(seen.remove(0)),
        _ => {
            eprintln!(
                "nebula: warning — mixed Go package names in {}: {:?}; using first ('{}')",
                parent.display(),
                seen,
                seen[0]
            );
            Some(seen.remove(0))
        }
    }
}

/// Sanitize a directory name into a Go-compatible package identifier.
/// Strips non-alphanumeric characters, lowercases, guards against reserved
/// keywords and empty results.
fn sanitize_go_package_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();

    if cleaned.is_empty() {
        return "main".to_string();
    }

    // Leading digit is illegal for Go identifiers.
    let cleaned = if cleaned
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        format!("pkg{}", cleaned)
    } else {
        cleaned
    };

    // Guard against Go reserved words.
    const GO_KEYWORDS: &[&str] = &[
        "break", "case", "chan", "const", "continue", "default", "defer",
        "else", "fallthrough", "for", "func", "go", "goto", "if", "import",
        "interface", "map", "package", "range", "return", "select", "struct",
        "switch", "type", "var",
    ];
    if GO_KEYWORDS.contains(&cleaned.as_str()) {
        format!("{}_", cleaned)
    } else {
        cleaned
    }
}

/// Convert Nebula to target language with graceful fallback.
///
/// Tries the full lex→parse→codegen pipeline first. If parsing fails
/// (e.g., ingester produced unparseable Nebula), falls back to a
/// best-effort text-level keyword replacement.
pub fn from_nebula_lenient(nebula: &str, lang: Language) -> Result<String, String> {
    // Try the proper pipeline first
    match from_nebula(nebula, lang) {
        Ok(output) => Ok(output),
        Err(_) => {
            // Text-level fallback: replace Nebula keywords with target equivalents
            Ok(text_level_translate(nebula, lang))
        }
    }
}

/// Best-effort text-level translation from Nebula to a target language.
/// Not AST-aware, but token-boundary-aware: replacements only fire at word
/// boundaries and skip the interiors of string literals. This is what stops
/// `("rt ", "return ")` from mangling `import` into `imporeturn`.
fn text_level_translate(nebula: &str, lang: Language) -> String {
    // Replacements are now bare keywords (no trailing space) so the word-boundary
    // logic can decide whether to fire on its own.
    let replacements: &[(&str, &str)] = match lang {
        Language::Python => &[
            ("rt", "return"), ("fo", "for"), ("wl", "while"),
            ("fn", "def"), ("sf", "self"),
            ("tt", "True"), ("ff", "False"),
            ("lt", ""), ("mt", ""),
        ],
        Language::Go => &[
            ("def", "func"), ("fn", "func"),
            ("rt", "return"), ("fo", "for"), ("wl", "for"),
            ("sf", "this"),
            ("True", "true"), ("False", "false"), ("None", "nil"),
            // Primitive types. Word-boundary semantics of
            // replace_word_outside_strings let "Int64" stay distinct from "Int"
            // (see test `word_boundary_protects_partial_matches`), so ordering
            // doesn't matter. Keep this table in sync with GoGen::gen_type's
            // TypeExpr::Named arms in codegen.rs.
            ("Str", "string"), ("String", "string"),
            ("Int", "int"),
            ("Int8", "int8"), ("Int16", "int16"), ("Int32", "int32"), ("Int64", "int64"),
            ("Uint", "uint"),
            ("Uint8", "uint8"), ("Uint16", "uint16"), ("Uint32", "uint32"), ("Uint64", "uint64"),
            ("Byte", "byte"), ("Rune", "rune"),
            ("Bool", "bool"),
            ("Float", "float64"), ("Float32", "float32"), ("Float64", "float64"),
            ("Complex64", "complex64"), ("Complex128", "complex128"),
            ("Any", "interface{}"), ("Error", "error"),
            // Qualified-type merges mirror gen_type's reverse table.
            ("TimeTime", "time.Time"), ("TimeDuration", "time.Duration"),
            ("SyncMutex", "sync.Mutex"), ("SyncRWMutex", "sync.RWMutex"),
            ("SyncWaitGroup", "sync.WaitGroup"),
            ("StringsBuilder", "strings.Builder"),
        ],
        Language::JavaScript | Language::TypeScript => &[
            ("def", "function"), ("fn", "function"),
            ("rt", "return"), ("fo", "for"), ("wl", "while"),
            ("sf", "this"),
            ("True", "true"), ("False", "false"), ("None", "null"),
            ("and", "&&"), ("or", "||"), ("not", "!"),
        ],
        Language::Rust => &[
            ("def", "fn"),
            ("rt", "return"), ("fo", "for"), ("wl", "while"),
            ("sf", "self"),
            ("True", "true"), ("False", "false"),
            ("Str", "String"), ("Int", "i64"), ("Bool", "bool"), ("Float", "f64"),
        ],
        Language::C => &[
            ("def", ""), ("fn", ""),
            ("rt", "return"), ("fo", "for"), ("wl", "while"),
            ("True", "1"), ("False", "0"), ("None", "NULL"),
        ],
        Language::Swift => &[
            ("def", "func"), ("fn", "func"),
            ("rt", "return"), ("fo", "for"), ("wl", "while"),
            ("sf", "self"),
            ("True", "true"), ("False", "false"), ("None", "nil"),
        ],
        Language::Kotlin => &[
            ("def", "fun"), ("fn", "fun"),
            ("rt", "return"), ("fo", "for"), ("wl", "while"),
            ("sf", "this"),
            ("True", "true"), ("False", "false"), ("None", "null"),
        ],
        _ => &[],
    };

    let mut output = nebula.to_string();
    for (from, to) in replacements {
        output = replace_word_outside_strings(&output, from, to);
    }

    // Walrus binding `:=` is Nebula-only — drop it for Python (bare assignment),
    // keep it for Go (which uses := natively).
    if matches!(lang, Language::Python) {
        output = output.replace(" := ", " = ");
    }

    // Convert indent blocks to braces for brace-based languages
    if matches!(lang, Language::Go | Language::JavaScript | Language::TypeScript |
                      Language::Rust | Language::C | Language::Swift | Language::Kotlin) {
        output = indent_to_braces(&output);
    }

    output
}

#[inline]
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Replace `from` with `to` only at token boundaries and only outside string
/// literals. A token boundary means the character before the match is not a
/// word char (or the match is at the start of input), and the character after
/// the match is not a word char (or end of input). String literals are
/// detected by `"`/`'` quote pairs with `\` escaping.
pub(crate) fn replace_word_outside_strings(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let from_chars: Vec<char> = from.chars().collect();
    let mut out = String::with_capacity(text.len());

    let mut in_str = false;
    let mut str_char = '"';
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == str_char {
                in_str = false;
            }
            i += 1;
            continue;
        }

        if c == '"' || c == '\'' {
            in_str = true;
            str_char = c;
            out.push(c);
            i += 1;
            continue;
        }

        // Try to match `from` here
        if i + from_chars.len() <= chars.len()
            && chars[i..i + from_chars.len()] == from_chars[..]
        {
            let prev_is_word = i > 0 && is_word_char(chars[i - 1]);
            let next_pos = i + from_chars.len();
            let next_is_word = next_pos < chars.len() && is_word_char(chars[next_pos]);
            // Both ends of `from` are checked: only fire if neither neighbor
            // extends the match into a longer identifier.
            if !prev_is_word && !next_is_word {
                out.push_str(to);
                i = next_pos;
                continue;
            }
        }

        out.push(c);
        i += 1;
    }

    out
}

#[cfg(test)]
mod text_translate_tests {
    use super::*;

    #[test]
    fn rt_does_not_mangle_import() {
        let input = "import \"unicode\"\nfn foo():\n    rt 1";
        let output = text_level_translate(input, Language::Go);
        assert!(output.contains("import \"unicode\""), "import should survive: {}", output);
        assert!(output.contains("return 1"), "rt should still translate: {}", output);
    }

    #[test]
    fn replacements_skip_string_literals() {
        let input = "fn foo():\n    return \"rt is a keyword\"";
        let output = replace_word_outside_strings(input, "rt", "return");
        assert!(output.contains("\"rt is a keyword\""), "string content should be untouched");
    }

    #[test]
    fn word_boundary_protects_partial_matches() {
        // "fn" should not match inside "function" or "fnord"
        let input = "function fnord";
        let output = replace_word_outside_strings(input, "fn", "def");
        assert_eq!(output, "function fnord");
    }

    // Iter-24 Bug B regression guards: width-specific Nebula type tokens must
    // lower to their native Go counterparts when the AST pipeline falls back to
    // text-level translation.
    #[test]
    fn float64_lowers_in_go_text_fallback() {
        let input = "fn avg(x: Float64) -> Float64:\n    rt x";
        let output = text_level_translate(input, Language::Go);
        assert!(output.contains("float64"), "Float64 must lower to float64: {}", output);
        assert!(!output.contains("Float64"), "Float64 must not leak: {}", output);
    }

    #[test]
    fn int64_lowers_in_go_text_fallback() {
        let input = "lt x: Int64 = 0";
        let output = text_level_translate(input, Language::Go);
        assert!(output.contains("int64"), "Int64 must lower to int64: {}", output);
        assert!(!output.contains("Int64"), "Int64 must not leak: {}", output);
    }

    #[test]
    fn int_still_lowers_when_int64_is_present() {
        // Both Int and Int64 should survive side-by-side thanks to word boundaries.
        let input = "fn f(a: Int, b: Int64) -> Int:\n    rt a";
        let output = text_level_translate(input, Language::Go);
        assert!(output.contains("int64"), "Int64: {}", output);
        // After Int → int, we should have the literal `int`; `int64` contains
        // `int` as a prefix so this is a containment check, not a word-boundary
        // assertion.
        assert!(output.matches("int").count() >= 2, "Int should also lower: {}", output);
        assert!(!output.contains("Int64"), "leak: {}", output);
    }
}

/// Convert indent-based blocks to brace-based blocks (best-effort).
fn indent_to_braces(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let mut result = Vec::new();
    let mut indent_stack: Vec<usize> = vec![0];

    for (i, line) in lines.iter().enumerate() {
        let stripped = line.trim_start();
        if stripped.is_empty() {
            result.push(line.to_string());
            continue;
        }

        let indent = line.len() - stripped.len();

        // Close blocks when dedenting
        while indent_stack.last().map_or(false, |&top| indent < top) {
            indent_stack.pop();
            let close_indent = " ".repeat(*indent_stack.last().unwrap_or(&0));
            result.push(format!("{}}}", close_indent));
        }

        // If line ends with ':', convert to opening brace
        if stripped.ends_with(':') && !stripped.starts_with('#') && !stripped.starts_with("//") {
            let without_colon = &stripped[..stripped.len() - 1];
            let next_is_indented = if i + 1 < lines.len() {
                let next_stripped = lines[i + 1].trim_start();
                let next_indent = if next_stripped.is_empty() { indent + 1 } else { lines[i + 1].len() - next_stripped.len() };
                next_indent > indent
            } else {
                false
            };
            if next_is_indented {
                result.push(format!("{}{} {{", " ".repeat(indent), without_colon));
                let next_stripped = lines[i + 1].trim_start();
                let next_indent = if next_stripped.is_empty() { indent + 1 } else { lines[i + 1].len() - next_stripped.len() };
                indent_stack.push(next_indent);
            } else {
                // Empty body (next line not indented) — emit empty block
                result.push(format!("{}{} {{}}", " ".repeat(indent), without_colon));
            }
        } else {
            result.push(line.to_string());
        }
    }

    // Close remaining blocks
    while indent_stack.len() > 1 {
        indent_stack.pop();
        let close_indent = " ".repeat(*indent_stack.last().unwrap_or(&0));
        result.push(format!("{}}}", close_indent));
    }

    result.join("\n")
}

fn gen_target(program: &ast::Program, lang: Language) -> String {
    match lang {
        Language::Python => codegen::to_python(program),
        Language::JavaScript => codegen::to_javascript(program),
        Language::TypeScript => codegen::to_typescript(program),
        Language::Go => codegen::to_go(program),
        Language::Swift => codegen::to_swift(program),
        Language::Kotlin => codegen::to_kotlin(program),
        Language::Rust => codegen::to_rust_lang(program),
        Language::C => codegen::to_c(program),
        Language::Nebula | Language::Unknown => unreachable!(),
    }
}
