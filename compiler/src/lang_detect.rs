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
    match lang {
        Language::Python => ingest::from_python(source),
        Language::JavaScript => ingest::from_javascript(source),
        Language::TypeScript => ingest::from_typescript(source),
        Language::Go => ingest::from_go(source),
        Language::Swift => ingest::from_swift(source),
        Language::Kotlin => ingest::from_kotlin(source),
        Language::Rust => ingest::from_rust(source),
        Language::C => ingest::from_c(source),
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

    let tokens = lexer::tokenize(&expanded)
        .map_err(|e| format!("Lex error at {}:{}: {}", e.line, e.col, e.message))?;
    let program = parser::parse(tokens)
        .map_err(|e| format!("Parse error at {}:{}: {}", e.line, e.col, e.message))?;

    Ok(gen_target(&program, lang))
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
/// Not AST-aware — just keyword and syntax replacement.
fn text_level_translate(nebula: &str, lang: Language) -> String {
    let mut output = nebula.to_string();

    // Common keyword replacements
    let replacements: &[(&str, &str)] = match lang {
        Language::Python => &[
            ("rt ", "return "), ("fo ", "for "), ("wl ", "while "),
            ("fn ", "def "), ("sf.", "self."), ("sf,", "self,"),
            ("tt", "True"), ("ff", "False"), ("lt ", ""),
            ("mt ", ""), (":= ", "= "),
        ],
        Language::Go => &[
            ("def ", "func "), ("rt ", "return "), ("fo ", "for "),
            ("wl ", "while "), ("sf.", "this."), ("fn ", "func "),
            ("True", "true"), ("False", "false"), ("None", "nil"),
        ],
        Language::JavaScript | Language::TypeScript => &[
            ("def ", "function "), ("rt ", "return "), ("fo ", "for "),
            ("wl ", "while "), ("sf.", "this."), ("fn ", "function "),
            ("True", "true"), ("False", "false"), ("None", "null"),
            ("and ", "&& "), ("or ", "|| "), ("not ", "!"),
        ],
        Language::Rust => &[
            ("def ", "fn "), ("rt ", "return "), ("fo ", "for "),
            ("wl ", "while "), ("sf.", "self."), ("sf,", "self,"),
            ("True", "true"), ("False", "false"), ("None", "None"),
        ],
        Language::C => &[
            ("def ", ""), ("rt ", "return "), ("fo ", "for "),
            ("wl ", "while "), ("True", "1"), ("False", "0"),
            ("None", "NULL"), ("fn ", ""),
        ],
        Language::Swift => &[
            ("def ", "func "), ("rt ", "return "), ("fo ", "for "),
            ("wl ", "while "), ("sf.", "self."), ("fn ", "func "),
            ("True", "true"), ("False", "false"), ("None", "nil"),
        ],
        Language::Kotlin => &[
            ("def ", "fun "), ("rt ", "return "), ("fo ", "for "),
            ("wl ", "while "), ("sf.", "this."), ("fn ", "fun "),
            ("True", "true"), ("False", "false"), ("None", "null"),
        ],
        _ => &[],
    };

    for (from, to) in replacements {
        output = output.replace(from, to);
    }

    // Convert indent blocks to braces for brace-based languages
    if matches!(lang, Language::Go | Language::JavaScript | Language::TypeScript |
                      Language::Rust | Language::C | Language::Swift | Language::Kotlin) {
        output = indent_to_braces(&output);
    }

    output
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
            result.push(format!("{}{} {{", " ".repeat(indent), without_colon));
            // Next line should be indented — track it
            if i + 1 < lines.len() {
                let next_stripped = lines[i + 1].trim_start();
                let next_indent = lines[i + 1].len() - next_stripped.len();
                if next_indent > indent {
                    indent_stack.push(next_indent);
                }
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
