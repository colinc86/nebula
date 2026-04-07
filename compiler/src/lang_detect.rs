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
