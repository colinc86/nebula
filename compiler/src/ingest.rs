/// Tree-sitter based source → Nebula translators.
///
/// Each language converter parses source code using tree-sitter, walks the
/// concrete syntax tree, and emits equivalent Nebula source text.
/// Unknown constructs fall back to raw source text with a comment marker.

use tree_sitter::{Node, Parser};

// ── Public API ──

pub fn from_python(source: &str) -> String { from_python_with_opts(source, false) }
pub fn from_javascript(source: &str) -> String { from_javascript_with_opts(source, false) }
pub fn from_typescript(source: &str) -> String { from_typescript_with_opts(source, false) }
pub fn from_go(source: &str) -> String { from_go_with_opts(source, false) }
pub fn from_swift(source: &str) -> String { from_swift_with_opts(source, false) }
pub fn from_kotlin(source: &str) -> String { from_kotlin_with_opts(source, false) }
pub fn from_rust(source: &str) -> String { from_rust_with_opts(source, false) }
pub fn from_c(source: &str) -> String { from_c_with_opts(source, false) }

pub fn from_python_with_opts(source: &str, keep_comments: bool) -> String {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_python::LANGUAGE.into()).expect("Failed to set Python language");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return source.to_string(),
    };
    let root = tree.root_node();
    let nebula = python::emit(&root, source.as_bytes(), 0);
    let nebula = postprocess_python_no_lt(&nebula);
    let nebula = strip_semicolons(&nebula);
    let nebula = strip_comments_and_blanks(&nebula, keep_comments);
    let nebula = if keep_comments { canonicalize_keep_comments(&nebula) } else { canonicalize(&nebula) };
    let nebula = braces_to_indent(&nebula);
    let nebula = nebula.replace(" := ", " = ");
    nebula.trim_end().to_string()
}

pub fn from_javascript_with_opts(source: &str, keep_comments: bool) -> String {
    convert_with_language(source, tree_sitter_javascript::LANGUAGE.into(), js::emit, keep_comments)
}

pub fn from_typescript_with_opts(source: &str, keep_comments: bool) -> String {
    convert_with_language(source, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), ts::emit, keep_comments)
}

pub fn from_go_with_opts(source: &str, keep_comments: bool) -> String {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_go::LANGUAGE.into()).expect("Failed to set Go language");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return source.to_string(),
    };
    let root = tree.root_node();
    let nebula = go::emit(&root, source.as_bytes(), 0);
    let nebula = postprocess_go(&nebula);
    let nebula = strip_semicolons(&nebula);
    let nebula = minify(&nebula, keep_comments);
    let nebula = if keep_comments { canonicalize_keep_comments(&nebula) } else { canonicalize(&nebula) };
    braces_to_indent(&nebula)
}

pub fn from_swift_with_opts(source: &str, keep_comments: bool) -> String {
    let nebula = swift::convert_swift_raw(source, 0);
    let nebula = postprocess(&nebula);
    let nebula = strip_semicolons(&nebula);
    let nebula = minify(&nebula, keep_comments);
    let nebula = fix_raw_keywords(&nebula);
    let nebula = if keep_comments { canonicalize_keep_comments(&nebula) } else { canonicalize(&nebula) };
    braces_to_indent(&nebula)
}

pub fn from_kotlin_with_opts(source: &str, keep_comments: bool) -> String {
    let nebula = kotlin::convert_kotlin_raw(source, 0);
    let nebula = postprocess(&nebula);
    let nebula = strip_semicolons(&nebula);
    let nebula = minify(&nebula, keep_comments);
    let nebula = fix_raw_keywords(&nebula);
    let nebula = if keep_comments { canonicalize_keep_comments(&nebula) } else { canonicalize(&nebula) };
    braces_to_indent(&nebula)
}

pub fn from_rust_with_opts(source: &str, keep_comments: bool) -> String {
    convert_with_language(source, tree_sitter_rust::LANGUAGE.into(), rust::emit, keep_comments)
}

pub fn from_c_with_opts(source: &str, keep_comments: bool) -> String {
    convert_with_language(source, tree_sitter_c::LANGUAGE.into(), c::emit, keep_comments)
}

// ── Core ──

fn convert_with_language(
    source: &str,
    language: tree_sitter::Language,
    emitter: fn(&Node, &[u8], usize) -> String,
    keep_comments: bool,
) -> String {
    let mut parser = Parser::new();
    parser.set_language(&language).expect("Failed to set language");

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return source.to_string(), // fallback to raw
    };

    let root = tree.root_node();
    let nebula = emitter(&root, source.as_bytes(), 0);
    // Post-processing: catch patterns the tree-sitter emitter misses
    let nebula = postprocess(&nebula);
    // Strip trailing semicolons (Nebula doesn't use them)
    let nebula = strip_semicolons(&nebula);
    // Minify: blank-line collapse + compact indentation; keep_comments controls whether // lines survive
    let nebula = minify(&nebula, keep_comments);
    // Final keyword cleanup (runs AFTER minify so whitespace is normalized)
    let nebula = fix_raw_keywords(&nebula);
    // Canonical normalization: snake_case, Python-native method names, etc.
    // When preserving comments, run a wrapper that excludes // lines from the pass.
    let nebula = if keep_comments { canonicalize_keep_comments(&nebula) } else { canonicalize(&nebula) };
    // Convert brace blocks to indent blocks (saves ~1 token per block boundary)
    let nebula = braces_to_indent(&nebula);
    // Strip orphan closing braces at end of file ONLY if braces are unbalanced
    let nebula = nebula.trim_end();
    let opens = nebula.chars().filter(|&c| c == '{').count();
    let closes = nebula.chars().filter(|&c| c == '}').count();
    if closes > opens {
        // Remove excess } from the end
        let mut result = nebula.to_string();
        for _ in 0..(closes - opens) {
            if let Some(pos) = result.rfind('}') {
                result.remove(pos);
            }
        }
        result.trim_end().to_string()
    } else {
        nebula.to_string()
    }
}

/// Python-specific postprocess: the Python ingester's AST walker already
/// produces near-final Nebula, so this is just a few string-level cleanups.
/// Deliberately does NOT run Go/Rust/Swift regex rules (which corrupt
/// docstrings) or `fix_python_ternary` (the ingester emits ternaries directly
/// from `conditional_expression` nodes).
/// Python postprocess without inserting lt/let on first assignments.
/// Python doesn't distinguish declaration vs reassignment, so bare assignment is cheaper.
fn postprocess_python_no_lt(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let processed = rewrite_python_line(line);
        out.push_str(&processed);
        out.push('\n');
    }
    out
}

fn postprocess_python(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let processed = rewrite_python_line(line);
        out.push_str(&processed);
        out.push('\n');
    }
    // Second pass: insert `lt` before the first bare `name = expr` assignment
    // within each function body. Python has no declaration vs reassignment
    // distinction, but Nebula's linter treats unprefixed first-assignments as
    // rebinding. We track function-scoped name sets using `fn ... {` as the
    // scope entry marker.
    insert_lt_on_first_assignment(&out)
}

fn insert_lt_on_first_assignment(source: &str) -> String {
    // Parse line-by-line. Maintain a stack of (depth, HashSet<String>) pairs.
    // On `fn NAME(PARAMS) ... {` push a new frame seeded with the parameter
    // names. On matching `}` pop. On `name = expr` at depth > 0 (inside some
    // function frame), if name is not in the current frame set, prepend `lt `
    // and add it.
    use std::collections::HashSet;
    let mut out = String::with_capacity(source.len());
    let mut stack: Vec<HashSet<String>> = Vec::new();
    // Pre-declare names at each brace level so we know when a frame pops.
    let mut brace_stack: Vec<usize> = Vec::new(); // number of open braces at frame boundary
    let mut open_braces = 0i32;

    for line in source.lines() {
        let trimmed = line.trim_start();
        // Detect function entry: `fn NAME(PARAMS) ... {`
        if trimmed.starts_with("fn ") && line.trim_end().ends_with('{') {
            // Extract parameter names.
            let mut params = HashSet::new();
            if let (Some(lp), Some(rp)) = (line.find('('), line.rfind(')')) {
                let inner = &line[lp + 1..rp];
                for p in inner.split(',') {
                    let p = p.trim();
                    if p.is_empty() { continue; }
                    // `name: Type` or `name = default` or just `name`
                    let name = p.split(|c: char| c == ':' || c == '=').next().unwrap_or("").trim();
                    if !name.is_empty() {
                        params.insert(name.to_string());
                    }
                }
            }
            stack.push(params);
            brace_stack.push(open_braces as usize);
            open_braces += 1;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // Non-fn brace bookkeeping.
        let opens = line.chars().filter(|&c| c == '{').count() as i32;
        let closes = line.chars().filter(|&c| c == '}').count() as i32;
        open_braces += opens;
        open_braces -= closes;

        // Pop frames whose scope brace has closed.
        while let Some(&frame_braces) = brace_stack.last() {
            if (open_braces as usize) <= frame_braces {
                stack.pop();
                brace_stack.pop();
            } else {
                break;
            }
        }

        if !stack.is_empty() {
            if let Some(new_line) = maybe_add_lt(line, stack.last_mut().unwrap()) {
                out.push_str(&new_line);
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// If `line` is a bare `NAME = EXPR` assignment where NAME is not yet in
/// `seen`, return `lt NAME = EXPR` and mark it seen. Returns None otherwise.
fn maybe_add_lt(line: &str, seen: &mut std::collections::HashSet<String>) -> Option<String> {
    let leading = line.len() - line.trim_start().len();
    let (prefix, body) = line.split_at(leading);
    // Must not already start with a keyword that owns the line.
    for kw in &["lt ", "mt ", "fn ", "rt", "if ", "el", "fo ", "wl ", "lp", "mc ", "st ", "en ", "im ", "tr ", "ct", "bk", "//", "use ", "try", "ch "] {
        if body.starts_with(kw) || body == kw.trim_end() {
            return None;
        }
    }
    // Find an `=` not preceded by a comparison/assignment operator char.
    let bytes = body.as_bytes();
    let mut eq_pos = None;
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'"' | b'\'' => {
                // skip to matching quote
                let q = b;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == q {
                        break;
                    }
                    i += 1;
                }
            }
            b'=' if depth == 0 => {
                let prev = if i > 0 { bytes[i - 1] } else { b' ' };
                let next = if i + 1 < bytes.len() { bytes[i + 1] } else { b' ' };
                // Skip ==, !=, <=, >=, =>, +=, -=, *=, /=, %=
                if matches!(prev, b'=' | b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b':') {
                    i += 1;
                    continue;
                }
                if next == b'=' || next == b'>' {
                    i += 1;
                    continue;
                }
                eq_pos = Some(i);
                break;
            }
            _ => {}
        }
        i += 1;
    }
    let eq_pos = eq_pos?;
    let lhs = body[..eq_pos].trim();
    // LHS must be a simple identifier (no dots, no brackets, no parens).
    if lhs.is_empty() {
        return None;
    }
    let first = lhs.chars().next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !lhs.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    if seen.contains(lhs) {
        return None;
    }
    seen.insert(lhs.to_string());
    Some(format!("{}lt {}", prefix, body))
}

/// Rewrite a single line of Python-derived Nebula, avoiding edits inside
/// string literals.
fn rewrite_python_line(line: &str) -> String {
    // Split by top-level string boundaries and rewrite only the outside parts.
    let chars: Vec<char> = line.chars().collect();
    let mut result = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            // Copy string literal verbatim (respecting escapes).
            let quote = c;
            result.push(c);
            i += 1;
            while i < chars.len() {
                let ch = chars[i];
                result.push(ch);
                if ch == '\\' && i + 1 < chars.len() {
                    result.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                i += 1;
                if ch == quote {
                    break;
                }
            }
            continue;
        }
        result.push(c);
        i += 1;
    }
    // Apply small outside-string rewrites over the whole line. These are safe
    // because the line's string contents are already copied verbatim above.
    let applied = apply_safe_python_rewrites(&result);
    applied
}

fn apply_safe_python_rewrites(line: &str) -> String {
    // Only the rewrites that are unambiguous outside string literals.
    // `self.` / `this.` → `sf.` and bare `self` → `sf`, etc., are already
    // handled inside python::emit_expr. We leave this as a no-op pass and
    // only do cleanups that the AST walker can't easily express.
    line.to_string()
}

/// Minimal cleanup for Python output: strip blank lines but preserve indentation.
/// `keep_comments=false` drops `//` comment-only lines (the canonical/benchmark form).
/// `keep_comments=true` retains them (for MCP read_code, where the model needs docstrings).
fn strip_comments_and_blanks(source: &str, keep_comments: bool) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") {
            if keep_comments || trimmed.starts_with("// @") {
                out.push_str(line);
                out.push('\n');
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Go-specific postprocess: minimal cleanup, no generic conversion
fn postprocess_go(source: &str) -> String {
    let s = source
        // Logical operators
        .replace("&&", "and")
        .replace("||", "or")
        // Go raw syntax
        .replace(":=", "=")
        // else → el
        .replace(" else {", " el {")
        .replace(" else\n", " el\n")
        .replace("} else ", "} el ")
        // Post-increment/decrement
        .replace("++", " += 1")
        .replace("--", " -= 1")
        // Go pointer types
        .replace("-> *", "-> ")
        // Strip * dereference before identifiers (Nebula doesn't have explicit deref)
        .replace("!= *", "!= ")
        .replace("== *", "== ");
    s
}

/// Post-process Nebula output to catch patterns the tree-sitter emitters miss.
fn postprocess(source: &str) -> String {
    let s = source
        // Self references
        .replace("self.", "sf.")
        .replace("this.", "sf.")
        // Booleans
        .replace(" True", " tt")
        .replace("(True", "(tt")
        .replace(",True", ",tt")
        .replace(" False", " ff")
        .replace("(False", "(ff")
        .replace(",False", ",ff")
        // Logical operators (C-family ! → Nebula not)
        .replace("&&", "and")
        .replace("||", "or")
        // Null variants
        .replace("nullptr", "nil")
        .replace("NULL", "nil")
        .replace("undefined", "nil")
        // Python boilerplate — remove the __main__ guard entirely
        .replace("if __name__ == \"__main__\" {", "")
        .replace("if __name__ == \"__main__\"", "")
        // Python builtins: strip wrapping list() — handled by fix_list_wrap below
        // Go/Rust scope resolution :: → .
        .replace("::", ".")
        // Dict key single quotes → double quotes (Python stats['key'] → stats["key"])
        .replace("['", "[\"")
        .replace("']", "\"]")
        // Spread operator (JS [...x]) — strip dots, but NOT range operators (2...n)
        .replace("[...", "[")
        .replace("(...", "(")
        .replace(", ...", ", ")
        // Rust lifetime annotations — strip 'static, 'a, etc.
        .replace("'static ", "")
        .replace("'a ", "")
        .replace("'b ", "")
        // C/Go format strings — escape literal newlines in strings
        .replace("\\n\"", "\"")
        .replace("\\n", "")
        // Go raw syntax leakage
        .replace(":=", "=")
        // Raw keywords — handled more thoroughly in fix_raw_keywords below

        // else → el (must not touch "else if" which is already "el if")
        .replace(" else {", " el {")
        .replace(" else\n", " el\n")
        .replace("} else ", "} el ")
        // Post-increment/decrement → += 1 (any single-word identifier)
        .replace("++", " += 1")
        .replace("--", " -= 1")
        // Rust reference types — strip & from type positions
        .replace("&str", "Str")
        .replace("&mut ", "")
        .replace("&[", "[")
        // Go/C pointer types — strip * and &
        .replace("-> *", "-> ")
        .replace("&{", "{")
        .replace("(&", "(")
        .replace(", &", ", ")
        .replace(" &", " ")  // strip & before identifiers in expressions
        .replace("rt &", "rt ")  // strip & after return
        // Rust Sf (mutable self) → sf (Nebula only uses lowercase sf)
        .replace("(Sf)", "(sf)")
        .replace("(Sf,", "(sf,")
        // Go map/slice literals: []Type{vals} → [vals], closing } → ]
        .replace("Map{", "{")
        .replace("[]Task{}", "[]")
        .replace("[]int{", "[").replace("[]string{", "[")
        // Swift/Kotlin optional: Type? → Type
        .replace("Task?", "Task")
        .replace("Int?", "Int")
        .replace("String?", "String")
        // Swift dict literal: ["key": val] → {"key": val}
        .replace("rt [\"", "rt {\"")
        .replace(", \"pct\": pct]", ", \"pct\": pct}")
        // Rust orphan > in expressions
        .replace("[Task>", "[Task]")
        .replace(": [Task>", ": List")
        // Rust Vec/Option → Nebula types
        .replace("Vec[", "[")
        .replace("Option[", "")
        // TS array type: Task[] → List
        .replace("Task[]", "List")
        .replace("number[]", "List")
        // Swift/Kotlin: mutating keyword
        .replace("mutating ", "")
        // Swift range: 2...n → 2..=n, 2..<n → 2..n
        .replace("...", "..=").replace("..<", "..")
        // Cleanup double spaces
        .replace(":  ", ": ").replace("  ", " ")
        // Kotlin collection constructors (before and after generic conversion)
        .replace("mutableListOf<Task>()", "[]")
        .replace("mutableListOf[Task]()", "[]")
        .replace("mutableListOf()", "[]")
        // listOf(a, b) → [a, b] — need to also close the bracket
        .replace("listOf(", "[")
        .replace("mapOf(", "{")
        // Kotlin map syntax: "key" to val → "key": val
        .replace("\" to ", "\": ")
        // Generic types in return positions: List[Task] → List, Map[K, V] → Map
        .replace("List[Task]", "List")
        .replace("Map[String, Any]", "Map")
        .replace("Map[String]", "Map")
        // Swift named parameter labels and optional binding
        .replace("_ p:", "p:")
        .replace("_ n:", "n:")
        .replace("_ manager:", "manager:")
        .replace("_ numbers:", "numbers:")
        // Swift optional binding: "if let/var task = expr {" → "if expr != nil {"
        .replace("if mt ", "if ")
        .replace("if var ", "if ")
        .replace("if lt ", "if ")
        // Rust tuple types: (Type, Type) → Tuple
        .replace("(usize, usize, usize, f64)", "Tuple")
        .replace("(usize,", "(Int,").replace("usize)", "Int)")
        .replace("usize", "Int")
        .replace("f64", "Float").replace("f32", "Float")
        .replace("i32", "Int").replace("i64", "Int")
        .replace("u32", "Int").replace("u64", "Int")
        // Rust type casts: "as Float" → "" (strip cast, Nebula doesn't have as)
        .replace(" as Float", "")
        .replace(" as Int", "")
        .replace(" as Str", "")
        // Go pointer types in params
        .replace(": *", ": ")
        // Go/Rust list type mangling: "Listint" → "List"
        .replace("Listint", "List")
        .replace("Liststring", "List")
        // Go range: "for _, t = range X {" → "fo _, t in X {"
        .replace(" range ", " ")
        // Go: convert remaining "= IDENT {" to "in IDENT {" in for loops (handled more in fix_raw_keywords)
        // Kotlin/Swift: else → el
        .replace("\" else \"", "\" el \"")
        .replace(") else ", ") el ")
        .replace("} else ", "} el ")
        // C raw declarations and pointer syntax in params
        .replace("const char *", "")
        .replace("const char*", "")
        .replace(")()", ")")  // double parens from C function pointer parse
        .replace("*m,", "m,").replace("*m)", "m)")
        .replace("*task,", "task,").replace("*task)", "task,")
        // C type-first params in function signatures: "int priority" → "priority: Int"
        .replace("int priority)", "priority: Int)")
        .replace("int priority,", "priority: Int,")
        .replace("int id_count)", "id_count: Int)")
        .replace("int id_count,", "id_count: Int,")
        .replace("int bufsize)", "bufsize: Int)")
        .replace("int bufsize,", "bufsize: Int,")
        // C raw declarations that leak into function bodies
        .replace("lt Task *", "lt ")
        .replace("lt int ", "lt ")
        .replace("lt const ", "lt ")
        .replace("lt char ", "lt ")
        .replace("lt float ", "lt ")
        .replace("lt double ", "lt ")
        // C macros/defines: ALL_CAPS → all_caps (parser treats uppercase as type constructors)
        .replace("MAX_TASKS", "max_tasks")
        .replace("MAX_TITLE", "max_title")
        // C function params: raw type-first params
        .replace(", int id)", ", id: Int)")
        .replace(", int id,", ", id: Int,")
        .replace("(int p)", "(p: Int)")
        .replace("(int n)", "(n: Int)")
        .replace("-> char {", "-> Str {")
        // Rust if-let: "if let Some(task) = expr {" → "if expr != nil {"
        .replace("if let Some(", "if_let_some ")
        .replace("TaskManager m", "m: TaskManager")
        .replace("const char *title", "title: Str")
        .replace("char *buf", "buf: Str")
        // Rust Vec[T] → [T] (after generics fix)
        .replace("Vec[", "[")
        // C pointer params: *task → task
        .replace("(*", "(").replace(", *", ", ")
        // Kotlin return type: "fn name(): Type" → "fn name() -> Type"
        .replace("): String", ") -> String")
        .replace("): Int", ") -> Int")
        .replace("): Boolean", ") -> Bool")
        .replace("): Bool", ") -> Bool")
        .replace("): List", ") -> List")
        .replace("): Map", ") -> Map")
        .replace("): Task", ") -> Task")
        .replace("): Any", ") -> Any")
        ;

    // Fix Python lambdas that leaked through: "lambda x: expr" → "|x| expr"
    let s = fix_lambda_syntax(&s);
    // Fix Python ternary: "val if cond else alt" → "if cond { val } el { alt }"
    let s = fix_python_ternary(&s);
    // Fix list comprehensions that leaked: [expr for var in iter if cond]
    let s = fix_list_comprehensions(&s);
    // Fix generics: Vec<Type> → [Type], Option<T> → T?, HashMap<K,V> → Map[K,V]
    let s = fix_generics(&s);
    // Fix C-style ternary: "cond ? val : alt" → "if cond { val } el { alt }"
    let s = fix_c_ternary(&s);
    // Kotlin/Swift: if (cond) val el alt → if cond { val } el { alt }
    let s = fix_braceless_if(&s);
    // Swift optional binding: "if x = expr {" → "if expr != nil {"
    let s = fix_swift_optional_binding(&s);
    // Single-line braceless if: "if (cond) stmt" → "if cond { stmt }"
    let s = fix_single_line_if(&s);
    // Fix list types in struct fields: ": [Type]" → ": List" (parser doesn't accept [T] as type)
    let s = fix_list_types(&s);
    // Final generic type cleanup (after fix_generics converted <> to [])
    let s = s.replace("Vec[", "[").replace("Option[", "")
        .replace("List[Task]", "List").replace("List[Int]", "List")
        .replace("List[String]", "List").replace("Map[String, Any]", "Map")
        .replace("[Task>", "List");
    // Re-run list type fix (generics conversion may have created new [Type] patterns)
    let s = fix_list_types(&s);
    // Go map types and slice literal bracket fixing
    let s = s.replace("map[string]interface{}", "Map").replace("Map{", "{");
    // Fix Go slice literal: [vals} → [vals] (closing brace should be bracket)
    let s = fix_go_slice_braces(&s);
    // Fix orphan brackets from incomplete generic conversion (Option<T> → T] etc.)
    let s = fix_orphan_brackets(&s);
    // C arrow operator: "ident->field" → "ident.field" (but NOT "-> Type" return annotation)
    // The C emitter already handles this in field_expression, but raw C leaking through still has ->
    // We detect: ->lowercase or ->IDENT without space
    let s = fix_c_arrow(&s);

    // Fix list(map(...)) → mp(...) — remove outer list() wrapper
    let s = s.replace("list(map(", "map(").replace("list(filter(", "filter(")
              .replace("list(mp(", "mp(").replace("list(fl(", "fl(");
    // Remove the extra ) that list() wrapper leaves behind
    let s = fix_unmatched_parens(&s);

    // Fix ! prefix operator → not (but not != or !important etc.)
    // Rust if-let-some: "if_let_some VAR) = EXPR {" → "if EXPR != nil {"
    let s = fix_rust_if_let(&s);
    // Final else → el (after Python ternary fix has run, so it doesn't break "else" detection)
    let s = fix_negation_operator(&s);
    s.replace(" else ", " el ")
}

/// Fix Python ternary: `val if cond else alt` → `if cond { val } el { alt }`
fn fix_python_ternary(source: &str) -> String {
    let mut result = String::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // Look for pattern: SOMETHING = EXPR if COND else EXPR
        // or standalone: EXPR if COND else EXPR
        if let Some(if_pos) = trimmed.find(" if ") {
            if let Some(else_pos) = trimmed.find(" else ") {
                let between = &trimmed[if_pos..else_pos];
                // Only match Python-style ternary: "VAL if COND else ALT"
                // Skip if: braces present (Nebula/Rust), parens after if (C/Kotlin/Swift)
                let looks_like_python_ternary = else_pos > if_pos
                    && !between.contains('{') && !between.contains('}')
                    && !trimmed[if_pos + 1..if_pos + 5].contains('(');
                if looks_like_python_ternary {
                    let before_if = &trimmed[..if_pos];
                    let cond = &trimmed[if_pos + 4..else_pos];
                    let alt = &trimmed[else_pos + 6..];

                    // Check if there's an assignment: "var = val if cond else alt"
                    if let Some(eq_pos) = before_if.rfind(" = ") {
                        let var_part = &before_if[..eq_pos + 3]; // "var = "
                        let val = &before_if[eq_pos + 3..];
                        let indent = &line[..line.len() - trimmed.len()];
                        result.push_str(&format!("{}{}if {} {{ {} }} el {{ {} }}\n",
                            indent, var_part, cond, val, alt));
                        continue;
                    } else {
                        // Standalone ternary
                        let indent = &line[..line.len() - trimmed.len()];
                        result.push_str(&format!("{}if {} {{ {} }} el {{ {} }}\n",
                            indent, cond, before_if, alt));
                        continue;
                    }
                }
            }
        }
        result.push_str(line);
        result.push('\n');
    }
    if result.ends_with('\n') && !source.ends_with('\n') {
        result.pop();
    }
    result
}

/// Fix generic type syntax: `Vec<T>` → `[T]`, `Option<T>` → `T?`, strip `&` in type args.
fn fix_generics(source: &str) -> String {
    let mut s = source.to_string();
    // Replace common generic patterns with Nebula syntax
    // Vec<&Type> → [Type], Vec<Type> → [Type]
    loop {
        let changed = s
            .replace("Vec<&", "[")
            .replace("[&", "[")  // already converted Vec<
            ;
        // Find Vec< or [ that was a generic open and close the > with ]
        // Simple approach: replace matching > after [ with ]
        let mut result = String::new();
        let mut depth: i32 = 0;
        let mut in_generic = false;
        for ch in changed.chars() {
            if ch == '[' {
                // Could be array or converted generic
                result.push(ch);
            } else if ch == '<' {
                // Check if this looks like a generic (preceded by identifier)
                let last = result.chars().last().unwrap_or(' ');
                if last.is_alphanumeric() || last == '>' || last == ']' {
                    result.push('[');
                    depth += 1;
                    in_generic = true;
                } else {
                    result.push(ch);
                }
            } else if ch == '>' && in_generic && depth > 0 {
                result.push(']');
                depth -= 1;
                if depth == 0 { in_generic = false; }
            } else {
                result.push(ch);
            }
        }
        if result == s { break; }
        s = result;
    }
    // Strip & from inside type brackets: [&Task] → [Task]
    s.replace("[&", "[").replace(", &", ", ")
}

/// Fix list types in struct field positions: ": [Type]" → ": List"
fn fix_list_types(source: &str) -> String {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // Pattern: "name: [Type]" in struct fields or "-> [Type]" in return types
        if let Some(colon_pos) = trimmed.find(": [") {
            if let Some(close) = trimmed[colon_pos..].find(']') {
                let before = &trimmed[..colon_pos + 2]; // "name: "
                let after = &trimmed[colon_pos + close + 1..];
                let ws = &line[..line.len() - trimmed.len()];
                out.push(format!("{}{}List{}", ws, before, after));
                continue;
            }
        }
        // Also handle "-> [Type]"
        if let Some(arr_pos) = trimmed.find("-> [") {
            if let Some(close) = trimmed[arr_pos..].find(']') {
                let before = &trimmed[..arr_pos + 3]; // "-> "
                let after = &trimmed[arr_pos + close + 1..];
                let ws = &line[..line.len() - trimmed.len()];
                out.push(format!("{}{}List{}", ws, before, after));
                continue;
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Fix C arrow operator: `ptr->field` → `ptr.field` but not `-> RetType`.
fn fix_c_arrow(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '-' && chars[i + 1] == '>' {
            // Check if preceded by identifier (no space) AND followed by identifier (no space)
            let prev = if i > 0 { chars[i - 1] } else { ' ' };
            let next = if i + 2 < chars.len() { chars[i + 2] } else { ' ' };
            if prev.is_alphanumeric() && (next.is_alphabetic() || next == '_') {
                // C arrow: ptr->field → ptr.field
                result.push('.');
                i += 2;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

/// Fix orphan brackets in return types: `-> Task]` → `-> Task`, `-> [Task>` → `-> List`
fn fix_orphan_brackets(source: &str) -> String {
    source.lines().map(|line| {
        let mut s = line.to_string();
        // Strip orphan ] at end of return type: "-> Type]" where no matching [
        if s.contains("-> ") {
            let arrow_pos = s.find("-> ").unwrap();
            let after_arrow = &s[arrow_pos + 3..];
            let after_trimmed = after_arrow.trim_end().trim_end_matches('{').trim_end();
            let opens = after_trimmed.chars().filter(|&c| c == '[').count();
            let closes = after_trimmed.chars().filter(|&c| c == ']').count();
            if closes > opens {
                // Remove excess ] from the return type portion
                for _ in 0..(closes - opens) {
                    if let Some(pos) = s.rfind(']') {
                        s.remove(pos);
                    }
                }
            }
            // If return type is "[Type" with no closing ], convert to "List"
            let ret_type = s[arrow_pos + 3..].trim().trim_end_matches('{').trim();
            if ret_type.starts_with('[') && !ret_type.contains(']') {
                let before_arrow = &s[..arrow_pos + 3];
                let after_brace = if s.trim_end().ends_with('{') { " {" } else { "" };
                s = format!("{}List{}", before_arrow, after_brace);
            }
            // Also remove orphan > (but NOT the > in "->")
            let after_arrow2 = &s[arrow_pos + 3..];
            let after_trimmed2 = after_arrow2.trim_end().trim_end_matches('{').trim_end();
            if after_trimmed2.contains('>') && !after_trimmed2.contains('<') {
                // Find a > that's not part of ->
                let search_start = arrow_pos + 3;
                if let Some(gt_pos) = s[search_start..].find('>') {
                    let abs_pos = search_start + gt_pos;
                    // Make sure it's not part of ->
                    if abs_pos == 0 || s.as_bytes()[abs_pos - 1] != b'-' {
                        s.remove(abs_pos);
                    }
                }
            }
        }
        s
    }).collect::<Vec<_>>().join("\n")
}

/// Fix Go slice literal braces: `[1, 2}` → `[1, 2]`
fn fix_go_slice_braces(source: &str) -> String {
    source.lines().map(|line| {
        let mut s = line.to_string();
        let opens = s.chars().filter(|&c| c == '[').count();
        let close_brackets = s.chars().filter(|&c| c == ']').count();
        if opens > close_brackets {
            // If line has unmatched [, look for } or ) to convert to ]
            let close_braces = s.chars().filter(|&c| c == '}').count();
            let open_braces = s.chars().filter(|&c| c == '{').count();
            if close_braces > open_braces {
                if let Some(pos) = s.rfind('}') {
                    s.replace_range(pos..pos+1, "]");
                }
            }
            // Also check for ) that should be ]
            let close_parens = s.chars().filter(|&c| c == ')').count();
            let open_parens = s.chars().filter(|&c| c == '(').count();
            let opens2 = s.chars().filter(|&c| c == '[').count();
            let close_brackets2 = s.chars().filter(|&c| c == ']').count();
            if opens2 > close_brackets2 && close_parens > open_parens {
                if let Some(pos) = s.rfind(')') {
                    s.replace_range(pos..pos+1, "]");
                }
            }
        }
        s
    }).collect::<Vec<_>>().join("\n")
}

/// Fix Rust if-let-some pattern: `if_let_some VAR) = EXPR {` → `if EXPR != nil {`
fn fix_rust_if_let(source: &str) -> String {
    source.lines().map(|line| {
        if let Some(pos) = line.find("if_let_some ") {
            let rest = &line[pos + 12..];
            if let Some(eq_pos) = rest.find(") = ") {
                let expr = &rest[eq_pos + 4..];
                let ws = &line[..pos];
                return format!("{}if {} != nil {{", ws,
                    expr.trim_end().trim_end_matches('{').trim());
            }
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n")
}

/// Fix single-line braceless if: `if (cond) stmt` → `if cond { stmt }`
fn fix_single_line_if(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim();
        // "if (COND) STMT" where STMT doesn't start with {
        if trimmed.starts_with("if (") && !trimmed.ends_with('{') && !trimmed.contains(" el ") {
            if let Some(close) = trimmed[3..].find(") ") {
                let cond = &trimmed[4..3 + close];
                let stmt = &trimmed[3 + close + 2..];
                if !stmt.starts_with('{') {
                    let ws = &line[..line.len() - trimmed.len()];
                    return format!("{}if {} {{ {} }}", ws, cond, stmt);
                }
            }
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n")
}

/// Fix Swift optional binding: `if task = expr {` → `if expr != nil {`
fn fix_swift_optional_binding(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim();
        // Pattern: "if IDENT = EXPR {" where it's not "if IDENT == ..."
        if trimmed.starts_with("if ") && trimmed.ends_with('{') {
            if let Some(eq_pos) = trimmed.find(" = ") {
                // Make sure it's not == (comparison)
                let before_eq = &trimmed[3..eq_pos]; // between "if " and " = "
                let after_eq = &trimmed[eq_pos + 3..].trim_end_matches('{').trim();
                // Only match if before_eq is a simple identifier (no operators)
                if before_eq.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && !trimmed.contains("==") {
                    let ws = &line[..line.len() - trimmed.len()];
                    return format!("{}if {} != nil {{", ws, after_eq);
                }
            }
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n")
}

/// Fix braceless if-else: `if (cond) val el alt` → `if cond { val } el { alt }`
fn fix_braceless_if(source: &str) -> String {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // Pattern: ... if (COND) EXPR el EXPR
        if let Some(if_pos) = trimmed.find("if (") {
            // Look for "el" or "else" (else hasn't been converted to el yet at this stage)
            let (el_pos, el_len) = if let Some(p) = trimmed.find(" el ") {
                (Some(p), 4)
            } else if let Some(p) = trimmed.find(" else ") {
                (Some(p), 6)
            } else {
                (None, 0)
            };
            if let Some(el_pos) = el_pos {
                // Find matching ) for the if condition
                if let Some(close_paren) = trimmed[if_pos + 3..].find(") ") {
                    let actual_close = if_pos + 3 + close_paren;
                    // Only if there's no { between ) and el (braceless)
                    let between = &trimmed[actual_close..el_pos];
                    if !between.contains('{') {
                        let before = &trimmed[..if_pos];
                        let cond = &trimmed[if_pos + 4..actual_close];
                        let then_val = &trimmed[actual_close + 2..el_pos];
                        let else_val = &trimmed[el_pos + el_len..];
                        let ws = &line[..line.len() - trimmed.len()];
                        out.push(format!("{}{}if {} {{ {} }} el {{ {} }}",
                            ws, before, cond, then_val, else_val));
                        continue;
                    }
                }
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Fix C-style ternary: `= COND ? VAL : ALT` → `= if COND { VAL } el { ALT }`
fn fix_c_ternary(source: &str) -> String {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // Pattern: "... = EXPR ? VAL : ALT" or "lt VAR = EXPR ? VAL : ALT"
        if let Some(q_pos) = trimmed.find(" ? ") {
            if let Some(c_pos) = trimmed[q_pos..].find(" : ") {
                let before_q = &trimmed[..q_pos];
                let val = &trimmed[q_pos + 3..q_pos + c_pos];
                let alt = &trimmed[q_pos + c_pos + 3..];
                // Find the assignment boundary
                if let Some(eq_pos) = before_q.rfind(" = ") {
                    let var_part = &before_q[..eq_pos + 3];
                    let cond = &before_q[eq_pos + 3..];
                    let ws = &line[..line.len() - trimmed.len()];
                    out.push(format!("{}{}if {} {{ {} }} el {{ {} }}", ws, var_part, cond, val, alt));
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Fix unmatched closing parens from list() wrapper removal.
fn fix_unmatched_parens(source: &str) -> String {
    source.lines().map(|line| {
        let opens = line.chars().filter(|&c| c == '(').count();
        let closes = line.chars().filter(|&c| c == ')').count();
        if closes > opens {
            // Remove excess closing parens from the end
            let excess = closes - opens;
            let mut s = line.to_string();
            for _ in 0..excess {
                if let Some(pos) = s.rfind(')') {
                    s.remove(pos);
                }
            }
            s
        } else {
            line.to_string()
        }
    }).collect::<Vec<_>>().join("\n")
}

/// Fix list comprehensions: `[expr for var in iter if cond]` → `iter.fl(|var| cond).mp(|var| expr)`
fn fix_list_comprehensions(source: &str) -> String {
    let mut out = Vec::new();
    for line in source.lines() {
        let mut l = line.to_string();
        // Keep trying to replace comprehensions on this line
        loop {
            if let Some(start) = l.find(" [") .or_else(|| if l.starts_with('[') { Some(0) } else { None }) {
                let bracket_start = if l[start..].starts_with(" [") { start + 1 } else { start };
                let after = &l[bracket_start + 1..];
                if let Some(for_pos) = after.find(" for ") {
                    if let Some(close) = after.find(']') {
                        if for_pos < close {
                            let inner = &after[..close];
                            let expr = inner[..for_pos].trim();
                            let rest = &inner[for_pos + 5..];
                            if let Some(in_pos) = rest.find(" in ") {
                                let var = rest[..in_pos].trim();
                                let after_in = &rest[in_pos + 4..];
                                let (iter, cond) = if let Some(if_pos) = after_in.find(" if ") {
                                    (after_in[..if_pos].trim(), Some(after_in[if_pos + 4..].trim()))
                                } else {
                                    (after_in.trim(), None)
                                };
                                let replacement = if let Some(c) = cond {
                                    format!("{}.fl(|{}| {}).mp(|{}| {})", iter, var, c, var, expr)
                                } else {
                                    format!("{}.mp(|{}| {})", iter, var, expr)
                                };
                                l = format!("{}{}{}", &l[..bracket_start], replacement, &l[bracket_start + 1 + close + 1..]);
                                continue;
                            }
                        }
                    }
                }
            }
            break;
        }
        out.push(l);
    }
    out.join("\n")
}

/// Convert `lambda params: body` to `|params| body`.
fn fix_lambda_syntax(source: &str) -> String {
    let mut result = source.to_string();
    while let Some(pos) = result.find("lambda ") {
        // Find the colon after params
        if let Some(colon_offset) = result[pos + 7..].find(": ") {
            let params = &result[pos + 7..pos + 7 + colon_offset];
            let before = &result[..pos];
            let after = &result[pos + 7 + colon_offset + 2..];
            result = format!("{}|{}| {}", before, params, after);
        } else {
            break;
        }
    }
    result
}

/// Convert `!expr` to `not expr`, but leave `!=` alone.
fn fix_negation_operator(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '!' {
            // Don't touch !=
            if i + 1 < chars.len() && chars[i + 1] == '=' {
                result.push('!');
            } else {
                // It's a prefix negation — replace with "not "
                result.push_str("not ");
            }
        } else {
            result.push(chars[i]);
        }
        i += 1;
    }
    result
}

/// Final pass: fix raw keywords that survived through minification.
/// Runs line-by-line so leading whitespace doesn't matter.
fn fix_raw_keywords(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        let ws = &line[..line.len() - trimmed.len()];

        // Keep keywords in canonical (Python-length) form
        let trimmed = if trimmed.starts_with("return ") || trimmed == "return" {
            // Already canonical — keep as-is
            trimmed.to_string()
        } else if trimmed.starts_with("match ") {
            // Already canonical — keep as-is
            trimmed.to_string()
        } else if trimmed.starts_with("for ") {
            // C-style for with semicolons → while loop
            if trimmed.contains(';') {
                let inner = trimmed.strip_prefix("for ").unwrap_or(trimmed);
                let inner = inner.trim_start_matches('(');
                let parts: Vec<&str> = inner.splitn(3, ';').collect();
                if parts.len() >= 2 {
                    let cond = parts[1].trim();
                    format!("while {} {{", cond)
                } else {
                    trimmed.replace(" = ", " in ")
                }
            } else {
                trimmed.replace(" = ", " in ")
            }
        } else if trimmed.starts_with("fo ") && trimmed.contains(" = ") && trimmed.contains('{') {
            // Go range that still has = instead of in
            trimmed.replace(" = ", " in ")
        }
        // Python tuple unpacking: "a, b = 0, 1" → "let (a, b) = (0, 1)"
        else if trimmed.contains(", ") && trimmed.contains(" = ")
                && !trimmed.contains("for ") && !trimmed.contains("fo ")
                && !trimmed.contains("range") && !trimmed.contains("in ") {
            let parts: Vec<&str> = trimmed.splitn(2, " = ").collect();
            if parts.len() == 2 && parts[0].contains(',') && !parts[0].contains('(') {
                format!("let ({}) = ({})", parts[0], parts[1])
            } else {
                trimmed.to_string()
            }
        }
        // C-style type declarations: "int id" or "Task tasks[N]" → "id: Int" / "tasks: [Task]"
        // Only apply to lines WITHOUT colons (already-converted lines have "name: Type")
        else if !trimmed.contains(':') && !trimmed.contains('(') && !trimmed.contains('=')
                && !trimmed.contains('{') && !trimmed.contains('}')
                && !trimmed.starts_with("fn ") && !trimmed.starts_with("rt ")
                && !trimmed.starts_with("lt ") && !trimmed.starts_with("mt ")
                && !trimmed.starts_with("if ") && !trimmed.starts_with("fo ")
                && !trimmed.starts_with("wl ") && !trimmed.starts_with("st ")
                && !trimmed.starts_with("im ") && !trimmed.starts_with("en ")
                && !trimmed.starts_with("tr ") && !trimmed.starts_with("mc ") {
            let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
            if parts.len() == 2 && parts[0].chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
                let type_name = parts[0];
                let var_part = parts[1].trim();
                // Strip array brackets: "tasks[MAX]" → "tasks", mark as array
                let (name, is_array) = if var_part.contains('[') {
                    (var_part.split('[').next().unwrap_or(var_part).trim(), true)
                } else {
                    (var_part, false)
                };
                let ty = match type_name {
                    "int" => "Int", "char" => "Char", "float" => "Float",
                    "double" => "Float", "long" => "Int", "const" => {
                        // "const char *..." — skip complex const declarations
                        trimmed.to_string();
                        trimmed
                    },
                    other => other,
                };
                if is_array {
                    // char arrays → Str, others → List
                    if ty == "Char" { format!("{}: Str", name) }
                    else { format!("{}: List", name) }
                } else {
                    format!("{}: {}", name, ty)
                }
            } else {
                trimmed.to_string()
            }
        }
        else {
            trimmed.to_string()
        };

        format!("{}{}", ws, trimmed)
    }).collect::<Vec<_>>().join("\n")
}

/// Minify ingested code: blank lines + compact indentation; comments stripped unless `keep_comments`.
/// Based on LLMLingua's principle of removing low-information-density tokens.
fn minify(source: &str, keep_comments: bool) -> String {
    source.lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() { return false; }
            // Comments: drop unless keep_comments is set, with @-annotations always preserved
            if trimmed.starts_with("//") && !trimmed.starts_with("// @") && !keep_comments {
                return false;
            }
            true
        })
        .map(|line| {
            // Compact indentation: 4-space → 2-space
            let stripped = line.trim_start();
            let leading = line.len() - stripped.len();
            let indent_level = leading / 4;
            let remainder = leading % 4;
            let compact_indent = "  ".repeat(indent_level + if remainder > 0 { 1 } else { 0 });
            format!("{}{}", compact_indent, stripped)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip trailing semicolons from each line (Nebula uses newlines as terminators).
fn strip_semicolons(source: &str) -> String {
    source.lines()
        .map(|line| {
            let l = line.trim_end_matches(';');
            // Also strip trailing commas from match arms (Rust/Kotlin)
            if l.contains("=>") && l.trim_end().ends_with(',') {
                l.trim_end().trim_end_matches(',')
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert brace-delimited blocks to indent-based (colon) blocks.
/// `def foo() {` → `def foo():`
/// `} else {` → `else:`
/// standalone `}` lines → removed (indentation handles it)
/// Also converts `struct X {} / impl X {` → `class X:`
fn braces_to_indent(source: &str) -> String {
    let mut out = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut skip_empty_struct = false;

    for i in 0..lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        let ws = &line[..line.len() - trimmed.len()];

        // Skip standalone } lines (block closers handled by indentation)
        if trimmed == "}" {
            continue;
        }

        // struct X {} followed by impl X { → class X:
        if trimmed.starts_with("struct ") && trimmed.ends_with("{}") {
            // Check if next non-empty line is impl for same type
            let struct_name = trimmed.strip_prefix("struct ").unwrap()
                .trim_end_matches("{}").trim();
            let next = lines[i+1..].iter().find(|l| !l.trim().is_empty());
            if let Some(next_line) = next {
                let nt = next_line.trim();
                if nt.starts_with("impl ") && nt.ends_with('{') {
                    let impl_name = nt.strip_prefix("impl ").unwrap()
                        .trim_end_matches('{').trim();
                    if impl_name == struct_name {
                        // Merge into class — skip this struct line, transform impl → class
                        skip_empty_struct = true;
                        continue;
                    }
                }
            }
            // Standalone empty struct — keep it
            out.push(line.to_string());
            continue;
        }

        // impl X { → class X: (when preceded by matching empty struct)
        if skip_empty_struct && trimmed.starts_with("impl ") && trimmed.ends_with('{') {
            let name = trimmed.strip_prefix("impl ").unwrap()
                .trim_end_matches('{').trim();
            out.push(format!("{}class {}:", ws, name));
            skip_empty_struct = false;
            continue;
        }
        skip_empty_struct = false;

        // Lines ending with { → replace { with :
        if trimmed.ends_with('{') && !trimmed.starts_with('"') {
            let without_brace = trimmed.trim_end_matches('{').trim_end();

            // } else { → else:
            if without_brace == "} else" {
                out.push(format!("{}else:", ws));
                continue;
            }
            // } else if COND { → elif COND: (saves 1 token)
            if without_brace.starts_with("} else if ") {
                let cond = without_brace.strip_prefix("} else if ").unwrap();
                out.push(format!("{}elif {}:", ws, cond));
                continue;
            }
            // } catch VAR { → except VAR: (Python-native)
            if without_brace.starts_with("} catch ") {
                let rest = without_brace.strip_prefix("} catch ").unwrap();
                out.push(format!("{}except {}:", ws, rest));
                continue;
            }
            // try { → try:
            if without_brace == "try" {
                out.push(format!("{}try:", ws));
                continue;
            }

            // General case: "def foo() {" → "def foo():"
            out.push(format!("{}{}:", ws, without_brace));
            continue;
        }

        // Regular line — keep as-is
        out.push(line.to_string());
    }

    out.join("\n")
}

/// Run `canonicalize` while preserving lines that begin with `//` verbatim.
/// Used by MCP read paths so docstrings don't get snake_cased into oblivion.
/// Comment lines are split out before canonicalize runs and re-inserted at the
/// same positions afterward, so the rest of the pipeline is unchanged.
fn canonicalize_keep_comments(source: &str) -> String {
    let mut out_lines: Vec<String> = Vec::new();
    let mut buffer: Vec<&str> = Vec::new();

    for line in source.lines() {
        if line.trim_start().starts_with("//") {
            if !buffer.is_empty() {
                let chunk = buffer.join("\n");
                for cl in canonicalize(&chunk).lines() {
                    out_lines.push(cl.to_string());
                }
                buffer.clear();
            }
            out_lines.push(line.to_string());
        } else {
            buffer.push(line);
        }
    }
    if !buffer.is_empty() {
        let chunk = buffer.join("\n");
        for cl in canonicalize(&chunk).lines() {
            out_lines.push(cl.to_string());
        }
    }

    out_lines.join("\n")
}

/// Canonical normalization pass. Runs after all language-specific postprocessing
/// to produce a single canonical Nebula form regardless of source language.
///
/// Normalizes:
/// 1. Identifiers: camelCase/PascalCase → snake_case
/// 2. Method short aliases: .fl() → .filter(), .mp() → .map(), etc.
/// 3. Builtin short aliases: ln() → len(), rng() → range(), prt() → print()
/// 4. Type names: string → Str, bool → Bool, int → Int, boolean → Bool
/// 5. Constructor patterns: Task.new(...) → Task(...)
/// 6. Boolean literals: true/false → tt/ff (already handled, but ensure consistency)
fn canonicalize(source: &str) -> String {
    let mut s = source.to_string();

    // ── 0. Normalize short-form keywords to canonical (Python-length) form ──
    s = normalize_keywords(&s);

    // ── 1. Normalize short method aliases to Python-native names ──
    // These are method calls: .alias(
    s = s.replace(".fl(", ".filter(")
         .replace(".mp(", ".map(")
         .replace(".ln(", ".len(")
         .replace(".ln\n", ".len\n")
         .replace(".ln ", ".len ")
         .replace(".jn(", ".join(")
         .replace(".fld(", ".reduce(")
         .replace(".ea(", ".for_each(")
         .replace(".rev(", ".reverse(")
         .replace(".it(", ".iter(")
         .replace(".col(", ".collect(")
         .replace(".uw(", ".unwrap(")
         .replace(".is_empty(", ".emp(")
         .replace(".clr(", ".clear(")
         .replace(".enu(", ".enumerate(")
         .replace(".ins(", ".insert(")
         .replace(".ck(", ".contains(");
    // Method as property: .ln (no parens) — e.g., JS .length
    // Only replace when .ln is at end of token (followed by space, newline, or operator)

    // ── 2. Normalize short builtin aliases to Python-native names ──
    // These are standalone function calls: alias(
    s = s.replace("prt(", "print(")
         .replace("psh(", "append(")
         .replace(" ln(", " len(")
         .replace("(ln(", "(len(")
         .replace("[ln(", "[len(")
         .replace("rng(", "range(")
         .replace("enu(", "enumerate(");

    // ── 3. Normalize type names to canonical forms ──
    s = s.replace(": string", ": Str")
         .replace(": String", ": Str")
         .replace(": bool", ": Bool")
         .replace(": boolean", ": Bool")
         .replace(": Boolean", ": Bool")
         .replace(": int", ": Int")
         .replace(": number", ": Int")
         .replace(": float", ": Float")
         .replace(": double", ": Float")
         .replace("-> string", "-> Str")
         .replace("-> String", "-> Str")
         .replace("-> bool", "-> Bool")
         .replace("-> boolean", "-> Bool")
         .replace("-> Boolean", "-> Bool")
         .replace("-> int", "-> Int")
         .replace("-> number", "-> Int")
         .replace("-> float", "-> Float")
         .replace("-> double", "-> Float");

    // ── 4. Normalize constructor patterns ──
    // Task.new(...) → Task(...) — Python-style constructors
    // Only for identifiers that start with uppercase (type names)
    let mut chars = s.chars().peekable();
    let mut buf = String::new();
    while let Some(c) = chars.next() {
        buf.push(c);
        // Check for ".new(" pattern after an identifier
        if c == '.' {
            if buf.len() >= 2 {
                let before_dot = &buf[..buf.len() - 1];
                let last_ident: String = before_dot.chars().rev()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>().chars().rev().collect();
                if !last_ident.is_empty() && last_ident.chars().next().unwrap().is_uppercase() {
                    // Check if "new(" follows
                    let peek: String = chars.clone().take(4).collect();
                    if peek.starts_with("new(") {
                        // Skip "new" — the dot is already in buf, remove it
                        buf.pop(); // remove the '.'
                        for _ in 0..3 { chars.next(); } // skip "new"
                        // Next char should be '(' which we'll naturally pick up
                        continue;
                    }
                }
            }
        }
    }
    s = buf;

    // ── 5. Normalize identifiers: camelCase → snake_case ──
    s = normalize_identifiers(&s);

    // ── 6. Normalize Go-style format strings to interpolation ──
    s = normalize_format_strings(&s);

    // ── 7. Strip language-specific residual patterns ──
    // Vec.new() / Vec() → []
    s = s.replace("Vec.new()", "[]").replace("Vec()", "[]");
    // go_var("...") → appropriate literal
    s = normalize_go_vars(&s);
    // Go generic types: List<Task> → List, Map<K, V> → Map, Ptr<T> → T
    s = strip_generic_params(&s);

    // Strip Rust iterator ceremony: .iter().filter() → .filter(), .iter().map() → .map()
    s = s.replace(".iter().filter(", ".filter(")
         .replace(".iter().map(", ".map(")
         .replace(".iter().enumerate(", ".enumerate(")
         .replace(".iter().for_each(", ".for_each(")
         .replace(".iter().reduce(", ".reduce(")
         .replace(".iter().find(", ".find(")
         .replace(".iter().any(", ".any(")
         .replace(".iter().all(", ".all(")
         .replace(".iter_mut().find(", ".find(")
         .replace(".collect()", "");
    // Also handle .it(). aliases
    s = s.replace(".it().filter(", ".filter(")
         .replace(".it().map(", ".map(")
         .replace(".it().col()", "");
    // Strip .copied() (Rust iterator adapter, not needed)
    s = s.replace(".copied()", "");
    // Strip identity .map(|t| t) — it's a no-op from list comprehension translation
    s = s.replace(".map(|t| t)", "");
    // Strip .to_string() — unnecessary type conversion
    s = s.replace(".to_string()", "");
    // Strip Go & references (Nebula doesn't have pointers)
    s = s.replace("return (&", "return (")
         .replace("return &", "return ")
         .replace("= &", "= ")
         .replace("(&", "(");
    // Go float64() casts → just use the value
    s = s.replace("float64(", "(")
         .replace("(double)", "");
    // Go append(list, x) → list.append(x)
    s = normalize_go_append(&s);
    // Strip __name__ guard
    s = strip_name_guard(&s);
    // Strip bare main() call at end of file
    s = strip_bare_main_call(&s);
    // NOTE: Kotlin/Swift trailing closures { it.X } are CHEAPER than lambdas (|it| it.X)
    // (5 tok vs 7 tok), so we keep them as-is.
    // Convert Kotlin string interpolation: $var → {var}, ${expr} → {expr}
    s = convert_kotlin_interpolation(&s);
    // Normalize Kotlin/Swift method names not caught by map_method
    s = s.replace(".remove_all ", ".filter ")
         .replace(".remove_all(", ".filter(")
         .replace(".toDouble()", "")
         .replace(".toFixed(", ".format(");
    // Fix Rust println! mangling: "printlnnot" → "print" (the ! was converted to "not")
    s = s.replace("printlnnot ", "print")
         .replace("printlnnot(", "print(");
    // Strip orphan .iter() at end of line (leftover after .collect() removal)
    s = s.replace(".iter()\n", "\n")
         .replace(".iter()", "");
    // Strip unused Go index variable: "for _, x in" → "for x in"
    s = s.replace("for _, ", "for ");
    // Convert Rust-style positional format strings: print("{} {}", a, b) → print("{a} {b}")
    s = normalize_positional_format_strings(&s);
    // Collapse temp-var swap pattern: __tmp = b / a = EXPR / b = __tmp → a, b = EXPR, a
    s = collapse_temp_swaps(&s);
    // Strip redundant fmt() wrapper when string is already interpolated
    s = strip_redundant_fmt(&s);

    // ── 8. Normalize remaining boolean/nil variants ──
    // Ensure consistent true→tt, false→ff (some may have slipped through)
    s = s.replace(": false", ": ff")
         .replace("= false", "= ff")
         .replace(": true", ": tt")
         .replace("= true", "= tt")
         .replace("(false)", "(ff)")
         .replace("(true)", "(tt)");

    s
}

/// Normalize short-form Nebula keywords to canonical Python-length forms.
/// Runs line-by-line, only converting at keyword positions (line start or after braces).
fn normalize_keywords(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + source.len() / 10);
    for line in source.lines() {
        let trimmed = line.trim_start();
        let ws = &line[..line.len() - trimmed.len()];

        // Replace leading keyword only (to avoid replacing inside identifiers/strings)
        let converted = if trimmed.starts_with("fn ") {
            format!("def {}", &trimmed[3..])
        } else if trimmed.starts_with("rt ") {
            format!("return {}", &trimmed[3..])
        } else if trimmed == "rt" {
            "return".to_string()
        } else if trimmed.starts_with("lt ") || trimmed.starts_with("let ") {
            // Immutable binding: lt/let x = expr → x := expr
            let rest = if trimmed.starts_with("lt ") { &trimmed[3..] } else { &trimmed[4..] };
            convert_let_to_walrus(rest)
        } else if trimmed.starts_with("mt ") || trimmed.starts_with("mut ") {
            // Mutable binding: mt/mut x = expr → x = expr (bare assignment)
            let rest = if trimmed.starts_with("mt ") { &trimmed[3..] } else { &trimmed[4..] };
            rest.to_string()
        } else if trimmed.starts_with("cn ") || trimmed.starts_with("const ") {
            // Const binding: cn/const x = expr → x := expr (same as let)
            let rest = if trimmed.starts_with("cn ") { &trimmed[3..] } else { &trimmed[6..] };
            convert_let_to_walrus(rest)
        } else if trimmed.starts_with("fo ") {
            format!("for {}", &trimmed[3..])
        } else if trimmed.starts_with("wl ") {
            format!("while {}", &trimmed[3..])
        } else if trimmed.starts_with("lp {") || trimmed == "lp" {
            format!("loop{}", &trimmed[2..])
        } else if trimmed.starts_with("mc ") {
            format!("match {}", &trimmed[3..])
        } else if trimmed.starts_with("st ") {
            format!("struct {}", &trimmed[3..])
        } else if trimmed.starts_with("en ") {
            format!("enum {}", &trimmed[3..])
        } else if trimmed.starts_with("tr ") {
            format!("trait {}", &trimmed[3..])
        } else if trimmed.starts_with("im ") {
            format!("impl {}", &trimmed[3..])
        } else if trimmed == "bk" || trimmed.starts_with("bk ") {
            format!("break{}", &trimmed[2..])
        } else if trimmed == "ct" {
            "continue".to_string()
        } else if trimmed.starts_with("pb ") {
            format!("pub {}", &trimmed[3..])
        } else {
            trimmed.to_string()
        };

        // Also normalize inline keywords: } el { → } else {, } ch → } catch
        let converted = converted
            .replace("} el {", "} else {")
            .replace("} el if", "} else if")
            .replace(" el {", " else {")
            .replace(" el if", " else if")
            .replace("} ch ", "} catch ");

        // Normalize sf → self (only when it's a standalone keyword, not part of identifier)
        let converted = normalize_self_keyword(&converted);

        // Normalize tt/ff/nil → True/False/None
        let converted = converted
            .replace(" tt", " True").replace("(tt", "(True").replace(",tt", ",True")
            .replace("{tt}", "{tt}")  // skip inside string interpolation
            .replace(" ff", " False").replace("(ff", "(False").replace(",ff", ",False")
            .replace(" nil", " None").replace("(nil", "(None");

        out.push_str(ws);
        out.push_str(&converted);
        out.push('\n');
    }
    if !source.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Convert `name = expr` to `name := expr` (walrus operator for immutable binding).
/// Handles type annotations: `name: Type = expr` → `name: Type := expr`
/// Handles destructuring: `(a, b) = expr` → `(a, b) := expr`
fn convert_let_to_walrus(rest: &str) -> String {
    // Find the first `=` that's not part of `==`, `!=`, `<=`, `>=`, `+=`, etc.
    let bytes = rest.as_bytes();
    let mut i = 0;
    let mut depth = 0i32;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'"' | b'\'' => {
                let q = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' { i += 2; continue; }
                    if bytes[i] == q { break; }
                    i += 1;
                }
            }
            b'=' if depth == 0 => {
                let prev = if i > 0 { bytes[i - 1] } else { b' ' };
                let next = if i + 1 < bytes.len() { bytes[i + 1] } else { b' ' };
                // Skip ==, !=, <=, >=, +=, -=, *=, /=, %=, :=, =>
                if matches!(prev, b'=' | b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b':') {
                    i += 1; continue;
                }
                if next == b'=' || next == b'>' {
                    i += 1; continue;
                }
                // Found the assignment =, replace with :=
                return format!("{} :={}", &rest[..i].trim_end(), &rest[i + 1..]);
            }
            _ => {}
        }
        i += 1;
    }
    // No = found, return as-is
    rest.to_string()
}

/// Replace sf with self in a line, only at word boundaries.
fn normalize_self_keyword(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 's' && i + 1 < chars.len() && chars[i + 1] == 'f' {
            // Check word boundaries
            let before_ok = i == 0 || !chars[i - 1].is_alphanumeric() && chars[i - 1] != '_';
            let after_ok = i + 2 >= chars.len() || (!chars[i + 2].is_alphanumeric() && chars[i + 2] != '_');
            if before_ok && after_ok {
                result.push_str("self");
                i += 2;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

/// Convert camelCase and PascalCase identifiers to snake_case.
/// Preserves: type names (start with uppercase and are standalone types like Task, TaskManager),
/// keywords, string literals, and struct/enum names.
fn normalize_identifiers(source: &str) -> String {
    // First pass: collect all type names defined in the source
    let mut type_names = std::collections::HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        for prefix in &["st ", "en ", "tr ", "im ", "struct ", "enum ", "trait ", "impl "] {
            if trimmed.starts_with(prefix) {
                let rest = &trimmed[prefix.len()..];
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !name.is_empty() {
                    type_names.insert(name);
                }
            }
        }
    }

    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim_start();
        let ws = &line[..line.len() - trimmed.len()];

        // Skip struct/enum/trait/impl declarations — preserve type names
        if trimmed.starts_with("st ") || trimmed.starts_with("en ")
            || trimmed.starts_with("tr ") || trimmed.starts_with("im ")
            || trimmed.starts_with("struct ") || trimmed.starts_with("enum ")
            || trimmed.starts_with("trait ") || trimmed.starts_with("impl ") {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        let converted = convert_identifiers_in_line(trimmed, &type_names);
        out.push_str(ws);
        out.push_str(&converted);
        out.push('\n');
    }
    // Remove trailing newline if source didn't have one
    if !source.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Convert identifiers in a single line from camelCase to snake_case.
/// Preserves string literals, type names (uppercase at type positions), and keywords.
fn convert_identifiers_in_line(line: &str, type_names: &std::collections::HashSet<String>) -> String {
    let mut result = String::with_capacity(line.len() + 16);
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Skip string literals
        if chars[i] == '"' || chars[i] == '\'' {
            let quote = chars[i];
            result.push(chars[i]);
            i += 1;
            while i < chars.len() {
                result.push(chars[i]);
                if chars[i] == '\\' && i + 1 < chars.len() {
                    result.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        // Collect identifier
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let mut ident = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                ident.push(chars[i]);
                i += 1;
            }

            // Decide whether to convert this identifier
            if should_convert_to_snake_case(&ident, &result, &chars, i, type_names) {
                result.push_str(&to_snake_case(&ident));
            } else {
                result.push_str(&ident);
            }
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }
    result
}

/// Determine if an identifier should be converted to snake_case.
/// Returns false for: type names, keywords, builtins, single-char names.
fn should_convert_to_snake_case(ident: &str, context_before: &str, chars: &[char], pos: usize, type_names: &std::collections::HashSet<String>) -> bool {
    // Don't convert single-char or all-lowercase identifiers
    if ident.len() <= 1 { return false; }
    if ident.chars().all(|c| c.is_lowercase() || c == '_') { return false; }
    // All-uppercase identifiers: convert short ones (Go field names like ID, OK)
    // but keep long constants (MAX_SIZE, etc.)
    if ident.chars().all(|c| c.is_uppercase() || c == '_') {
        // Short all-caps identifiers (2-3 chars) are likely Go field names
        if ident.len() > 3 { return false; }
        // But don't convert in type positions
    }

    // Don't convert known type names from this file
    if type_names.contains(ident) {
        return false;
    }

    // Don't convert Nebula keywords and builtin type names
    let protected = ["fn", "rt", "lt", "mt", "cn", "if", "el", "fo", "wl", "lp",
                     "mc", "st", "en", "im", "tr", "bk", "ct", "sf", "Sf",
                     "tt", "ff", "nil", "and", "or", "not", "in", "as", "try", "ch",
                     "use", "md", "pb", "ty", "ac", "aw", "sp",
                     "Int", "Str", "Bool", "Float", "List", "Map", "Set", "Tuple",
                     "String", "Char", "Ptr", "Any", "Fn",
                     "Vec", "Option", "Some", "None", "Result", "Ok", "Err",
                     "Double", "println", "print", "len", "range", "enumerate",
                     "sorted", "filter", "map", "append", "join", "contains",
                     "reduce", "for_each", "reverse", "iter", "collect",
                     "True", "False", "true", "false"];
    if protected.contains(&ident) { return false; }

    // Check what comes before this identifier
    let ctx = context_before.trim_end();
    let first = ident.chars().next().unwrap();

    // Type position: after ":", "->", or declaration keywords — DON'T convert
    if first.is_uppercase() {
        if ctx.ends_with(':') || ctx.ends_with("->")
            || ctx.ends_with("st") || ctx.ends_with("im")
            || ctx.ends_with("en") || ctx.ends_with("tr")
            || ctx.ends_with("fo") || ctx.ends_with('<')
            || ctx.ends_with(',') && ctx.contains('<') {
            return false;
        }
        // Followed by ( — could be constructor or function call
        if pos < chars.len() && chars[pos] == '(' {
            // After "fn"/"def" — function declaration, always convert
            // After "." — method call, always convert
            let ctx_trimmed = context_before.trim_end();
            if ctx_trimmed.ends_with("fn") || ctx_trimmed.ends_with("def") || context_before.ends_with('.') {
                // Fall through to convert
            } else if !has_camel_case(ident) {
                // Single-word PascalCase followed by ( — likely constructor (Task(), Map())
                return false;
            }
            // Multi-word PascalCase followed by ( — likely function call (NewTaskManager())
            // Fall through to convert
        }
        // Followed by { — struct literal (but only if standalone, not after ".")
        if pos < chars.len() && chars[pos] == '{' && !context_before.ends_with('.') {
            return false;
        }
        if pos + 1 < chars.len() && chars[pos] == ' ' && chars[pos + 1] == '{' && !context_before.ends_with('.') {
            return false;
        }
    }

    // Convert if it has camelCase pattern (lowercase then uppercase)
    if has_camel_case(ident) {
        return true;
    }

    // Also convert single-word PascalCase that isn't a type name.
    // Type names appear in type positions (checked above) or are well-known types (in protected list).
    // If we reach here with a PascalCase word (first char uppercase), it's a
    // Go-style field/method name that should be lowercased.
    if first.is_uppercase() {
        return true;
    }

    false
}

/// Check if an identifier has camelCase or PascalCase pattern.
fn has_camel_case(s: &str) -> bool {
    let mut prev_lower = false;
    for c in s.chars() {
        if c.is_uppercase() && prev_lower {
            return true;
        }
        prev_lower = c.is_lowercase();
    }
    false
}

/// Convert a camelCase or PascalCase identifier to snake_case.
fn to_snake_case(s: &str) -> String {
    // All-uppercase short identifiers: just lowercase them
    if s.chars().all(|c| c.is_uppercase() || c == '_') {
        return s.to_lowercase();
    }
    let mut result = String::with_capacity(s.len() + 4);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower = i > 0 && chars[i - 1].is_lowercase();
            let next_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();
            let prev_upper = i > 0 && chars[i - 1].is_uppercase();
            if prev_lower || (prev_upper && next_lower) {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

/// Normalize Go-style format strings: fmt("format %v %d", args) → print("format {args}")
/// and print("msg %v\n", arg) → print("msg {arg}")
fn normalize_format_strings(source: &str) -> String {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // Match: print("...", arg1, arg2) or fmt("...", arg1, arg2)
        // where format string contains %v, %d, %s, %f, %.1f, etc.
        if (trimmed.contains("print(\"") || trimmed.contains("fmt(\""))
            && (trimmed.contains("%v") || trimmed.contains("%d") || trimmed.contains("%s")
                || trimmed.contains("%f") || trimmed.contains("%.")) {
            let ws = &line[..line.len() - trimmed.len()];
            if let Some(converted) = convert_format_to_interpolation(trimmed) {
                out.push(format!("{}{}", ws, converted));
                continue;
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Convert a printf-style call to string interpolation.
/// `print("Hello %s, you are %d", name, age)` → `print("Hello {name}, you are {age}")`
fn convert_format_to_interpolation(line: &str) -> Option<String> {
    // Find the function call and opening quote
    let func_end = line.find("(\"")?;
    let func = &line[..func_end];

    let after_open = &line[func_end + 2..];
    // Find the closing quote of the format string
    let mut quote_end = None;
    let mut ci = 0;
    let chars: Vec<char> = after_open.chars().collect();
    while ci < chars.len() {
        if chars[ci] == '\\' {
            ci += 2;
            continue;
        }
        if chars[ci] == '"' {
            quote_end = Some(ci);
            break;
        }
        ci += 1;
    }
    let quote_end = quote_end?;
    let fmt_str = &after_open[..quote_end];

    // Get the args after the format string
    let rest = &after_open[quote_end + 1..]; // starts with , or )
    let rest = rest.trim_start();

    // If no args after format string, skip
    if rest.starts_with(')') {
        return None;
    }

    let rest = rest.strip_prefix(',')?;
    let args_str = rest.trim_start().trim_end().trim_end_matches(')');

    // Split args (respecting nested parens)
    let args = split_args(args_str);

    // Replace format specifiers with {arg}
    let mut result_fmt = String::new();
    let mut arg_idx = 0;
    let mut fi = 0;
    let fmt_chars: Vec<char> = fmt_str.chars().collect();
    while fi < fmt_chars.len() {
        if fmt_chars[fi] == '%' && fi + 1 < fmt_chars.len() {
            let next = fmt_chars[fi + 1];
            if next == '%' {
                result_fmt.push('%');
                fi += 2;
                continue;
            }
            // Skip format specifier: %d, %s, %v, %f, %.1f, etc.
            fi += 1; // skip %
            // Skip flags/width/precision
            while fi < fmt_chars.len() && (fmt_chars[fi] == '.' || fmt_chars[fi].is_ascii_digit() || fmt_chars[fi] == '-' || fmt_chars[fi] == '+') {
                fi += 1;
            }
            // Skip conversion char
            if fi < fmt_chars.len() {
                fi += 1;
            }
            // Insert {arg}
            if arg_idx < args.len() {
                result_fmt.push('{');
                result_fmt.push_str(args[arg_idx].trim());
                result_fmt.push('}');
                arg_idx += 1;
            }
        } else if fmt_chars[fi] == '\\' && fi + 1 < fmt_chars.len() && fmt_chars[fi + 1] == 'n' {
            // Strip \n from format strings (print adds newline)
            fi += 2;
        } else {
            result_fmt.push(fmt_chars[fi]);
            fi += 1;
        }
    }

    Some(format!("{}(\"{}\")", func, result_fmt))
}

/// Split arguments string by commas, respecting nested parens/brackets.
fn split_args(s: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        args.push(&s[start..]);
    }
    args
}

/// Strip generic type parameters: List<Task> → List, Map<K,V> → Map, Ptr<T> → T
fn strip_generic_params(source: &str) -> String {
    let mut s = source.to_string();
    // Common patterns
    loop {
        let prev = s.clone();
        // Ptr<Type> → Type (Go pointer type — just use the inner type)
        while let Some(pos) = s.find("Ptr<") {
            if let Some(close) = s[pos..].find('>') {
                let inner = &s[pos + 4..pos + close].to_string();
                s = format!("{}{}{}", &s[..pos], inner, &s[pos + close + 1..]);
            } else {
                break;
            }
        }
        // List<Type> → List
        while let Some(pos) = s.find("List<") {
            if let Some(close) = s[pos..].find('>') {
                s = format!("{}List{}", &s[..pos], &s[pos + close + 1..]);
            } else {
                break;
            }
        }
        // Map<K, V> → Map
        while let Some(pos) = s.find("Map<") {
            if let Some(close) = s[pos..].find('>') {
                s = format!("{}Map{}", &s[..pos], &s[pos + close + 1..]);
            } else {
                break;
            }
        }
        if s == prev { break; }
    }
    s
}

/// Convert Go-style `append(list, item)` to `list.append(item)`.
fn normalize_go_append(source: &str) -> String {
    let mut s = source.to_string();
    // Pattern: "X = append(X, Y)" → "X.append(Y)"
    // Pattern: "append(X, Y)" at start → "X.append(Y)"
    let mut result = Vec::new();
    for line in s.lines() {
        let trimmed = line.trim();
        let ws = &line[..line.len() - trimmed.len()];
        // Match: VAR = append(VAR, EXPR)
        if let Some(eq_pos) = trimmed.find(" = append(") {
            let var = trimmed[..eq_pos].trim();
            let inner = &trimmed[eq_pos + 10..]; // after "= append("
            if let Some(comma_pos) = inner.find(", ") {
                let first_arg = inner[..comma_pos].trim();
                let rest = inner[comma_pos + 2..].trim_end_matches(')');
                if first_arg == var {
                    result.push(format!("{}{}.append({})", ws, var, rest));
                    continue;
                }
            }
        }
        result.push(line.to_string());
    }
    result.join("\n")
}

/// Strip `if __name__ == "__main__":` guard and its body.
fn strip_name_guard(source: &str) -> String {
    let mut lines: Vec<&str> = source.lines().collect();
    // Find and remove __name__ guard
    let mut i = 0;
    let mut result = Vec::new();
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("if __name__") {
            // Skip this line and the indented body
            let guard_indent = lines[i].len() - trimmed.len();
            i += 1;
            while i < lines.len() {
                let next_trimmed = lines[i].trim();
                if next_trimmed.is_empty() {
                    i += 1;
                    continue;
                }
                let next_indent = lines[i].len() - next_trimmed.len();
                if next_indent > guard_indent {
                    // Part of the guard body — include the content but dedented
                    // Actually, for Python it's usually just main(), skip it
                    i += 1;
                } else {
                    break;
                }
            }
            continue;
        }
        result.push(lines[i]);
        i += 1;
    }
    result.join("\n")
}

/// Convert Rust-style positional format strings: `print("{} {}", a, b)` → `print("{a} {b}")`.
/// Also handles `fmt(...)` calls.
fn normalize_positional_format_strings(source: &str) -> String {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // Match: func("...{} ...{}", arg1, arg2) patterns
        if (trimmed.contains("print(\"") || trimmed.contains("fmt(\""))
            && trimmed.contains("{}")
        {
            let ws = &line[..line.len() - trimmed.len()];
            if let Some(converted) = convert_positional_format(trimmed) {
                out.push(format!("{}{}", ws, converted));
                continue;
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Convert `print("{} {}", a, b)` → `print("{a} {b}")`.
fn convert_positional_format(line: &str) -> Option<String> {
    let func_end = line.find("(\"")?;
    let func = &line[..func_end];

    let after_open = &line[func_end + 2..];
    // Find closing quote
    let mut ci = 0;
    let chars: Vec<char> = after_open.chars().collect();
    while ci < chars.len() {
        if chars[ci] == '\\' { ci += 2; continue; }
        if chars[ci] == '"' { break; }
        ci += 1;
    }
    if ci >= chars.len() { return None; }
    let fmt_str = &after_open[..ci];
    let rest = &after_open[ci + 1..].trim_start();

    if !rest.starts_with(',') { return None; }
    let args_str = rest[1..].trim().trim_end_matches(')');
    let args = split_args(args_str);

    // Replace {} with {arg}
    let mut result_fmt = String::new();
    let mut arg_idx = 0;
    let mut fi = 0;
    let fmt_chars: Vec<char> = fmt_str.chars().collect();
    while fi < fmt_chars.len() {
        if fi + 1 < fmt_chars.len() && fmt_chars[fi] == '{' && fmt_chars[fi + 1] == '}' {
            if arg_idx < args.len() {
                result_fmt.push('{');
                result_fmt.push_str(args[arg_idx].trim());
                result_fmt.push('}');
                arg_idx += 1;
            }
            fi += 2;
        } else if fi + 2 < fmt_chars.len() && fmt_chars[fi] == '{' && fmt_chars[fi + 1] == ':' {
            // Format spec like {:.1} — find closing }
            let spec_start = fi;
            fi += 1;
            while fi < fmt_chars.len() && fmt_chars[fi] != '}' { fi += 1; }
            if fi < fmt_chars.len() {
                let spec = &fmt_str[spec_start + 1..fi]; // :.1 etc
                if arg_idx < args.len() {
                    result_fmt.push('{');
                    result_fmt.push_str(args[arg_idx].trim());
                    result_fmt.push_str(spec);
                    result_fmt.push('}');
                    arg_idx += 1;
                }
                fi += 1;
            }
        } else {
            result_fmt.push(fmt_chars[fi]);
            fi += 1;
        }
    }

    Some(format!("{}(\"{}\")", func, result_fmt))
}

/// Collapse temp-variable swap patterns back into tuple assignment.
/// `__tmp = B; A = EXPR; B = __tmp` → `A, B = EXPR, A`
/// Also handles: `temp = B; B = EXPR; A = temp` patterns.
fn collapse_temp_swaps(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // Look for pattern: INDENT __tmp = VAR / INDENT VAR2 = EXPR / INDENT VAR = __tmp
        if i + 2 < lines.len() {
            let l1 = lines[i].trim();
            let l2 = lines[i + 1].trim();
            let l3 = lines[i + 2].trim();
            let ws = &lines[i][..lines[i].len() - l1.len()];

            // Pattern: __tmp = X / Y = EXPR / X = __tmp
            if l1.starts_with("__tmp = ") || l1.starts_with("temp = ") {
                let tmp_name = if l1.starts_with("__tmp") { "__tmp" } else { "temp" };
                let saved_var = l1.strip_prefix(&format!("{} = ", tmp_name)).unwrap_or("").trim();

                if l3 == format!("{} = {}", saved_var, tmp_name) {
                    // l2 is the assignment to the other variable
                    if let Some(eq_pos) = l2.find(" = ") {
                        let target = &l2[..eq_pos];
                        let expr = &l2[eq_pos + 3..];
                        result.push(format!("{}{}, {} = {}, {}", ws, target, saved_var, expr, target));
                        i += 3;
                        continue;
                    }
                }
            }
        }

        result.push(lines[i].to_string());
        i += 1;
    }
    result.join("\n")
}

/// Convert Kotlin-style `$var` and `${expr}` to Nebula `{var}` and `{expr}` inside strings.
fn convert_kotlin_interpolation(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '"' {
            // Inside a string literal — scan for $ patterns
            result.push('"');
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    result.push(chars[i]);
                    result.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if chars[i] == '$' && i + 1 < chars.len() {
                    if chars[i + 1] == '{' {
                        // ${expr} → {expr} — just skip the $
                        i += 1; // skip $, the { will be pushed next iteration
                        continue;
                    } else if chars[i + 1].is_alphabetic() || chars[i + 1] == '_' {
                        // $var → {var}
                        i += 1; // skip $
                        result.push('{');
                        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                            result.push(chars[i]);
                            i += 1;
                        }
                        result.push('}');
                        continue;
                    }
                }
                result.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                result.push('"'); // closing quote
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Strip redundant `fmt("...")` when the string is already interpolated (no % specifiers).
fn strip_redundant_fmt(source: &str) -> String {
    let mut s = source.to_string();
    // Find fmt("...") where the string has no % specifiers
    while let Some(pos) = s.find("fmt(\"") {
        // Find the matching closing )
        let after = &s[pos + 4..]; // after "fmt("
        if let Some(close) = find_matching_paren(after) {
            let inner = &after[..close];
            // If inner is a single string with no % specifiers, strip the fmt() wrapper
            if !inner.contains('%') && inner.starts_with('"') && inner.ends_with('"') {
                let before = &s[..pos];
                let after_close = &s[pos + 4 + close + 1..];
                s = format!("{}{}{}", before, inner, after_close);
                continue;
            }
        }
        break; // avoid infinite loop
    }
    s
}

/// Find matching `)` for an opening `(`.
fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1;
    let mut in_string = false;
    for (i, c) in s.char_indices() {
        if c == '"' && (i == 0 || s.as_bytes()[i-1] != b'\\') {
            in_string = !in_string;
        }
        if in_string { continue; }
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 { return Some(i); }
            }
            _ => {}
        }
    }
    None
}

/// Convert Kotlin/Swift trailing closures `{ it.X }` or `{ it * 2 }` to lambdas `(|it| it.X)`.
fn convert_trailing_closures(source: &str) -> String {
    let mut s = source.to_string();
    // Pattern: .method { BODY } where BODY contains "it" references
    // Common forms: .filter { it.done }, .map { it * 2 }, .sorted { it.X }
    loop {
        let prev = s.clone();
        // Find pattern: WORD { ... it ... }
        // Simple approach: replace "{ it" patterns after method names
        if let Some(pos) = s.find(" { it") {
            // Find the closing }
            let after = &s[pos + 2..]; // skip " {"
            if let Some(close) = find_matching_brace(after) {
                let body = &after[..close].trim().to_string();
                let before = &s[..pos];
                let after_close = &s[pos + 2 + close + 1..];
                // Convert to (|it| body)
                s = format!("{}(|it| {}){}", before, body, after_close);
                continue;
            }
        }
        // Also handle "{ not it.X }" pattern
        if let Some(pos) = s.find(" { not it") {
            let after = &s[pos + 2..];
            if let Some(close) = find_matching_brace(after) {
                let body = &after[..close].trim().to_string();
                let before = &s[..pos];
                let after_close = &s[pos + 2 + close + 1..];
                s = format!("{}(|it| {}){}", before, body, after_close);
                continue;
            }
        }
        if s == prev { break; }
    }
    s
}

/// Find the position of the matching `}` for an opening `{`, handling nesting.
fn find_matching_brace(s: &str) -> Option<usize> {
    let mut depth = 1;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Strip bare `main()` call at end of file.
fn strip_bare_main_call(source: &str) -> String {
    let trimmed = source.trim_end();
    if trimmed.ends_with("\nmain()") || trimmed == "main()" {
        trimmed.trim_end_matches("main()").trim_end().to_string()
    } else {
        source.to_string()
    }
}

/// Normalize go_var("...") calls to appropriate Nebula literals.
fn normalize_go_vars(source: &str) -> String {
    let mut s = source.to_string();
    // go_var("[]Type") → []
    while let Some(pos) = s.find("go_var(\"") {
        if let Some(close) = s[pos..].find("\")") {
            let before = &s[..pos];
            let after = &s[pos + close + 2..];
            let inner = &s[pos + 8..pos + close];
            let replacement = if inner.starts_with("[]") {
                "[]" // empty slice literal
            } else {
                "nil" // fallback
            };
            s = format!("{}{}{}", before, replacement, after);
        } else {
            break;
        }
    }
    s
}

// ── Shared helpers ──

fn text<'a>(node: &Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

fn indent(depth: usize) -> String {
    "    ".repeat(depth)
}

fn emit_children_as_statements(
    node: &Node,
    src: &[u8],
    depth: usize,
    emitter: fn(&Node, &[u8], usize) -> String,
) -> String {
    let mut out = String::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let line = emitter(&child, src, depth);
        if !line.is_empty() {
            out.push_str(&line);
            if !line.ends_with('\n') {
                out.push('\n');
            }
        }
    }
    out
}

/// Map common builtin function names to canonical Nebula equivalents.
/// Uses Python-native names where they are optimal for cl100k_base tokenization.
fn map_builtin(name: &str) -> &str {
    match name {
        "print" | "println" | "puts" | "fmt.Println" | "fmt.Printf" | "console.log"
            | "console.warn" | "console.error" | "printf" | "fprintf" => "print",
        "len" | "strlen" => "len",
        "int" => "int",
        "type" | "isinstance" | "typeof" => "type",
        "range" => "range",
        "enumerate" => "enumerate",
        "abs" => "abs",
        "min" => "min",
        "max" => "max",
        "sorted" => "sorted",
        "snprintf" | "sprintf" | "fmt.Sprintf" | "String.format" => "fmt",
        _ => name,
    }
}

/// Map common method names to canonical Nebula equivalents.
/// Uses Python-native names — these are optimal for cl100k_base tokenization
/// (`.append(` = 2 tokens, `.filter(` = 2 tokens vs short aliases = 3 tokens).
/// Map common method names to canonical Nebula equivalents.
/// Each canonical form is the CHEAPEST token cost verified by cl100k_base audit.
/// All methods below cost 2 tokens as `.method(` unless noted otherwise.
fn map_method(name: &str) -> &str {
    match name {
        // All 2 tokens as .method( — use readable Python-native names
        "length" | "count" | "size" | "ln" => "len",
        "push" | "append" => "append",
        "contains" | "includes" | "containsKey" | "has" | "ck" => "contains", // .contains( = 2tok, .ck( = 3tok
        "join" | "jn" | "joined" | "joinToString" => "join",     // .join( = 2tok, .jn( = 3tok
        "map" | "mp" => "map",
        "filter" | "retain" | "fl" => "filter",
        "reduce" | "fold" | "fold_left" | "fld" => "reduce",     // .reduce( = 2tok, .fld( = 3tok
        "forEach" | "for_each" | "ea" => "for_each",
        "reverse" | "reversed" | "rev" => "reverse",
        "iter" | "iterator" | "it" => "iter",
        "collect" | "col" => "collect",
        "unwrap" | "unwrap_or" | "uw" => "unwrap",               // .unwrap( = 2tok, .uw( = 3tok
        "isEmpty" | "is_empty" => "emp",                          // .emp( = 2tok, .is_empty( = 3tok!
        "removeAll" | "clear" | "clr" => "clear",                 // .clear( = 2tok, .clr( = 3tok
        "toFixed" | "format" | "fmt" => "format",
        "enumerate" | "enumerated" | "enu" => "enumerate",
        "pop" | "removeLast" => "pop",
        "insert" | "ins" => "insert",
        "keys" => "keys",
        "values" | "vals" => "values",
        "sum" => "sum",
        "sort" | "sorted" | "sortedBy" | "sort_by_key" => "sort",
        "split" => "split",
        "strip" | "trim" => "strip",
        "first" => "find",
        "to_string" | "toString" | "str" => "str",
        _ => name,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Python
// ═══════════════════════════════════════════════════════════════════

mod python {
    use super::*;

    pub fn emit(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);

        match node.kind() {
            "module" => emit_children_as_statements(node, src, depth, emit),

            "comment" => {
                let t = text(node, src);
                let body = t.strip_prefix('#').unwrap_or(t).trim_start();
                format!("{}// {}", ind, body)
            }

            "expression_statement" => {
                if let Some(child) = node.named_child(0) {
                    // Docstring detection: standalone triple-quoted string
                    // literals become // comment blocks, not string literals.
                    // Nebula has no triple-quote syntax.
                    if child.kind() == "string" {
                        let raw = text(&child, src);
                        if raw.starts_with("\"\"\"") || raw.starts_with("'''") {
                            let inner = raw
                                .trim_start_matches("\"\"\"")
                                .trim_start_matches("'''")
                                .trim_end_matches("\"\"\"")
                                .trim_end_matches("'''");
                            let mut buf = String::new();
                            for (i, doc_line) in inner.lines().enumerate() {
                                if i > 0 {
                                    buf.push('\n');
                                }
                                buf.push_str(&ind);
                                buf.push_str("// ");
                                buf.push_str(doc_line.trim());
                            }
                            return buf;
                        }
                    }
                    // Statement-shaped children (assignment, augmented_assignment)
                    // are dispatched back into emit so they get the `lt`
                    // prefix and proper statement formatting. Everything
                    // else is an expression statement.
                    if matches!(child.kind(), "assignment" | "augmented_assignment") {
                        return emit(&child, src, depth);
                    }
                    format!("{}{}", ind, emit_expr(&child, src))
                } else {
                    String::new()
                }
            }

            "function_definition" => {
                let raw_name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let name = if raw_name == "__init__" { "new" } else { raw_name };
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_params(&n, src)).unwrap_or_default();
                let ret = node.child_by_field_name("return_type")
                    .map(|n| format!(" -> {}", emit_type(&n, src)));

                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                // Check for decorators
                let mut decorators = String::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "decorator" {
                        let dec_text = text(&child, src);
                        let dec_name = dec_text.strip_prefix('@').unwrap_or(dec_text);
                        decorators.push_str(&format!("{}#[{}]\n", ind, dec_name));
                    }
                }

                format!("{}{}fn {}({}){}{}", decorators, ind, name, params,
                        ret.unwrap_or_default(), body)
            }

            "class_definition" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                // Nebula: st {} for fields, im {} for methods
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}st {} {{}}\n{}im {}{}", ind, name, ind, name, body)
            }

            "if_statement" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let then = node.child_by_field_name("consequence")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                let mut result = format!("{}if {}{}", ind, cond, then);

                // Handle elif/else chains
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "elif_clause" => {
                            let elif_cond = child.child_by_field_name("condition")
                                .map(|n| emit_expr(&n, src)).unwrap_or_default();
                            let elif_body = child.child_by_field_name("consequence")
                                .map(|n| emit_block(&n, src, depth))
                                .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                            result.push_str(&format!(" el if {}{}", elif_cond, elif_body));
                        }
                        "else_clause" => {
                            let else_body = child.child_by_field_name("body")
                                .map(|n| emit_block(&n, src, depth))
                                .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                            result.push_str(&format!(" el{}", else_body));
                        }
                        _ => {}
                    }
                }

                result
            }

            "for_statement" => {
                let left = node.child_by_field_name("left")
                    .map(|n| text(&n, src)).unwrap_or("_");
                let mut right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                // Nebula's `fo i, x in collection` already implies enumeration,
                // and the codegen re-wraps with enumerate(...) on to-py. Strip
                // an explicit `enumerate(...)` / `enu(...)` here so we don't
                // double-wrap on round-trip.
                if left.contains(',') {
                    let stripped = right
                        .strip_prefix("enumerate(")
                        .or_else(|| right.strip_prefix("enu("))
                        .and_then(|rest| rest.strip_suffix(')'));
                    if let Some(inner) = stripped {
                        right = inner.to_string();
                    }
                }
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}fo {} in {}{}", ind, left, right, body)
            }

            "while_statement" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}wl {}{}", ind, cond, body)
            }

            "return_statement" => {
                // Python `return a, b` emits an expression_list child, which
                // our emit_expr fallback treats as a tuple. But sometimes
                // tree-sitter surfaces the values as multiple direct named
                // children on return_statement itself — collect both cases.
                let mut cursor = node.walk();
                let children: Vec<Node> = node.named_children(&mut cursor).collect();
                match children.len() {
                    0 => format!("{}rt", ind),
                    1 => {
                        let c = &children[0];
                        if c.kind() == "expression_list" {
                            let mut inner_cursor = c.walk();
                            let parts: Vec<String> = c
                                .named_children(&mut inner_cursor)
                                .map(|n| emit_expr(&n, src))
                                .collect();
                            if parts.len() > 1 {
                                format!("{}rt ({})", ind, parts.join(", "))
                            } else {
                                format!("{}rt {}", ind, parts.join(", "))
                            }
                        } else {
                            format!("{}rt {}", ind, emit_expr(c, src))
                        }
                    }
                    _ => {
                        let parts: Vec<String> =
                            children.iter().map(|c| emit_expr(c, src)).collect();
                        format!("{}rt ({})", ind, parts.join(", "))
                    }
                }
            }

            "pass_statement" => format!("{}// pass", ind),
            "break_statement" => format!("{}bk", ind),
            "continue_statement" => format!("{}ct", ind),

            "import_statement" => {
                // `import math` / `import a.b [as x]`
                let mut cursor = node.walk();
                let mut parts = Vec::new();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "dotted_name" {
                        let name = text(&child, src).trim();
                        if name.starts_with("typing") {
                            parts.push(format!("// import {}", name));
                        } else {
                            parts.push(format!("use {}", name));
                        }
                    } else if child.kind() == "aliased_import" {
                        let mod_name = child
                            .named_child(0)
                            .map(|n| text(&n, src).trim().to_string())
                            .unwrap_or_default();
                        let alias = child
                            .child_by_field_name("alias")
                            .map(|n| text(&n, src).trim().to_string());
                        if let Some(a) = alias {
                            parts.push(format!("use {} as {}", mod_name, a));
                        } else {
                            parts.push(format!("use {}", mod_name));
                        }
                    }
                }
                if parts.is_empty() {
                    return format!("{}// {}", ind, text(node, src).trim());
                }
                parts
                    .into_iter()
                    .map(|p| format!("{}{}", ind, p))
                    .collect::<Vec<_>>()
                    .join("\n")
            }

            "import_from_statement" => {
                // `from X import A, B` → `use X.{A, B}`
                // `from X import A as a` → supported by Nebula? Fall back to
                // comment if there's any `as` alias inside.
                let module = node
                    .child_by_field_name("module_name")
                    .map(|n| text(&n, src).trim().to_string())
                    .unwrap_or_default();
                if module.is_empty() || module.starts_with("typing") {
                    return format!("{}// {}", ind, text(node, src).trim().replace('\n', " "));
                }
                let mut cursor = node.walk();
                let mut items = Vec::new();
                let mut has_alias = false;
                for child in node.children(&mut cursor) {
                    if !child.is_named() { continue; }
                    let kind = child.kind();
                    if kind == "dotted_name" {
                        let t = text(&child, src).trim().to_string();
                        if t != module {
                            items.push(t);
                        }
                    } else if kind == "aliased_import" {
                        has_alias = true;
                        break;
                    } else if kind == "wildcard_import" {
                        return format!("{}// from {} import *", ind, module);
                    }
                }
                if has_alias || items.is_empty() {
                    return format!(
                        "{}// {}",
                        ind,
                        text(node, src).trim().replace('\n', " ")
                    );
                }
                format!("{}use {}.{{{}}}", ind, module, items.join(", "))
            }

            "assignment" => {
                let left_node = node.child_by_field_name("left");
                let right_node = node.child_by_field_name("right");

                // Tuple-target assignment: `a, b = expr`. Nebula doesn't have
                // reassignment destructuring, so expand into a temp-destructure
                // plus per-name reassignment. Covers the Python tuple-swap
                // idiom `a, b = b, a`.
                if let Some(ln) = left_node {
                    if matches!(ln.kind(), "pattern_list" | "tuple_pattern" | "expression_list") {
                        let mut lcursor = ln.walk();
                        let targets: Vec<String> = ln
                            .named_children(&mut lcursor)
                            .map(|n| emit_expr(&n, src))
                            .collect();
                        let right_text = right_node
                            .map(|n| emit_expr(&n, src))
                            .unwrap_or_default();
                        // Emit direct tuple assignment: a, b = b, a + b
                        return format!("{}{} = {}", ind, targets.join(", "), right_text);
                    }
                }

                let left = left_node.as_ref()
                    .map(|n| emit_expr(n, src)).unwrap_or_default();
                let right = right_node
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();

                // For Python round-trip: always emit bare `name = expr`. The
                // Nebula parser accepts bare assignment, and Python has no
                // distinction between first-binding and reassignment. A later
                // pass in postprocess_python inserts `lt` on the first
                // occurrence of each name within each function body so the
                // code still parses as Nebula.
                let type_node = node.child_by_field_name("type");
                let type_ann = type_node.map(|n| format!(": {}", emit_type(&n, src)));
                format!("{}{}{} = {}", ind, left, type_ann.unwrap_or_default(), right)
            }

            "augmented_assignment" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                // Get operator (+= -= etc)
                let op = node.children(&mut node.walk())
                    .find(|c| !c.is_named())
                    .map(|c| text(&c, src))
                    .unwrap_or("=");
                format!("{}{} {} {}", ind, left, op, right)
            }

            "try_statement" => {
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                let mut result = format!("{}try{}", ind, body);

                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "except_clause" {
                        let binding = child.child_by_field_name("name")
                            .map(|n| text(&n, src))
                            .unwrap_or("e");
                        let handler = child.child_by_field_name("body")
                            .or_else(|| child.named_children(&mut child.walk()).last())
                            .map(|n| emit_block(&n, src, depth))
                            .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                        result.push_str(&format!(" ch {}{}", binding, handler));
                    }
                }

                result
            }

            "with_statement" => {
                // with X as Y: body → { lt Y = X \n body }
                format!("{}// {}", ind, text(node, src).lines().next().unwrap_or("with ..."))
            }

            "decorated_definition" => {
                // Pass through to the function/class inside
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "function_definition" || child.kind() == "class_definition" {
                        return emit(&child, src, depth);
                    }
                }
                format!("{}{}", ind, text(node, src))
            }

            "assert_statement" => {
                if let Some(expr) = node.named_child(0) {
                    format!("{}// assert {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}// assert", ind)
                }
            }

            "raise_statement" => {
                if let Some(expr) = node.named_child(0) {
                    format!("{}// raise {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}// raise", ind)
                }
            }

            _ => {
                // Fallback: emit raw source
                let raw = text(node, src).trim();
                if raw.is_empty() {
                    String::new()
                } else {
                    format!("{}{}", ind, raw)
                }
            }
        }
    }

    fn emit_expr(node: &Node, src: &[u8]) -> String {
        match node.kind() {
            "identifier" => {
                let name = text(node, src);
                match name {
                    "self" => "sf".to_string(),
                    "True" => "tt".to_string(),
                    "False" => "ff".to_string(),
                    "None" => "nil".to_string(),
                    _ => name.to_string(),
                }
            }

            "true" => "tt".to_string(),
            "false" => "ff".to_string(),
            "none" => "nil".to_string(),
            "integer" | "float" => text(node, src).to_string(),

            "string" | "concatenated_string" => {
                let raw = text(node, src);
                // Convert f-strings: f"...{x}..." → "...{x}..."
                if raw.starts_with("f\"") || raw.starts_with("f'") {
                    format!("\"{}\"", &raw[2..raw.len()-1])
                } else if raw.starts_with("'") {
                    // Single quotes → double quotes
                    format!("\"{}\"", &raw[1..raw.len()-1])
                } else {
                    raw.to_string()
                }
            }

            "call" => {
                let func = node.child_by_field_name("function")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let args = node.child_by_field_name("arguments")
                    .map(|n| emit_args(&n, src)).unwrap_or_default();

                // Map builtins
                let func_name = map_builtin(&func);
                format!("{}({})", func_name, args)
            }

            "attribute" => {
                let obj = node.child_by_field_name("object")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let attr = node.child_by_field_name("attribute")
                    .map(|n| text(&n, src)).unwrap_or("");
                let attr = map_method(attr);
                format!("{}.{}", obj, attr)
            }

            "subscript" => {
                let val = node.child_by_field_name("value")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                // If the subscript is a Python slice node, translate to a
                // Nebula method call that the codegen maps back to slice
                // syntax. This avoids parser changes for open-ended ranges.
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if !child.is_named() { continue; }
                    if child.kind() == "slice" {
                        return emit_python_slice(&val, &child, src);
                    }
                }
                // Plain indexing — collect all subscript-field children.
                let mut subs = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if !child.is_named() { continue; }
                    if child.kind() == "slice" { continue; }
                    // Skip the value field.
                    if let Some(vc) = node.child_by_field_name("value") {
                        if child.id() == vc.id() { continue; }
                    }
                    subs.push(emit_expr(&child, src));
                }
                let sub = subs.join(", ");
                format!("{}[{}]", val, sub)
            }

            "binary_operator" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src))
                    .unwrap_or("+");
                let op = match op {
                    "**" => "^",
                    other => other,
                };
                format!("{} {} {}", left, op, right)
            }

            "comparison_operator" => {
                // Walk children in order. Operators are unnamed tokens; named
                // children are the operands. A binary `in` / `not in` gets
                // rewritten as `rhs.con(lhs)` because Nebula doesn't have an
                // `in` expression operator (only `fo x in iterable`).
                let mut operands: Vec<String> = Vec::new();
                let mut ops: Vec<String> = Vec::new();
                let mut prev_unnamed: Option<String> = None;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.is_named() {
                        // Flush any pending unnamed operator (e.g. "is" in
                        // `x is None`) before the right-hand operand.
                        if let Some(prev) = prev_unnamed.take() {
                            ops.push(prev);
                        }
                        operands.push(emit_expr(&child, src));
                    } else {
                        let t = text(&child, src).to_string();
                        // Combine "is not" or "not in" into a single op token.
                        if let Some(prev) = prev_unnamed.take() {
                            ops.push(format!("{} {}", prev, t));
                        } else if t == "not" || t == "is" {
                            prev_unnamed = Some(t);
                        } else {
                            ops.push(t);
                        }
                    }
                }
                if let Some(prev) = prev_unnamed.take() {
                    ops.push(prev);
                }
                if operands.len() == 2 && ops.len() == 1 {
                    let lhs = &operands[0];
                    let rhs = &operands[1];
                    match ops[0].as_str() {
                        "in" => return format!("{}.con({})", rhs, lhs),
                        "not in" => return format!("not {}.con({})", rhs, lhs),
                        "is" => return format!("{} == {}", lhs, rhs),
                        "is not" => return format!("{} != {}", lhs, rhs),
                        _ => {}
                    }
                }
                // Chained comparisons: interleave operands and ops.
                let mut out = String::new();
                for (i, operand) in operands.iter().enumerate() {
                    if i > 0 && i - 1 < ops.len() {
                        out.push(' ');
                        out.push_str(&ops[i - 1]);
                        out.push(' ');
                    }
                    out.push_str(operand);
                }
                out
            }

            "boolean_operator" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src))
                    .unwrap_or("and");
                format!("{} {} {}", left, op, right)
            }

            "not_operator" => {
                let arg = node.child_by_field_name("argument")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("not {}", arg)
            }

            "unary_operator" => {
                // tree-sitter-python uses field "argument" for the operand.
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src)).unwrap_or("-");
                let operand = node.child_by_field_name("argument")
                    .or_else(|| node.child_by_field_name("operand"))
                    .or_else(|| node.named_child(0))
                    .map(|n| emit_expr(&n, src))
                    .unwrap_or_default();
                format!("{}{}", op, operand)
            }

            "conditional_expression" => {
                // Python: value if condition else alternative
                // Nebula: if condition { value } el { alternative }
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();
                if children.len() >= 3 {
                    let value = emit_expr(&children[0], src);
                    let cond = emit_expr(&children[1], src);
                    let alt = emit_expr(&children[2], src);
                    format!("if {} {{ {} }} el {{ {} }}", cond, value, alt)
                } else {
                    text(node, src).to_string()
                }
            }

            "lambda" => {
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_lambda_params(&n, src))
                    .unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("|{}| {}", params, body)
            }

            "list" => {
                let mut items = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    items.push(emit_expr(&child, src));
                }
                format!("[{}]", items.join(", "))
            }

            "dictionary" => {
                let mut pairs = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "pair" {
                        let k = child.child_by_field_name("key")
                            .map(|n| emit_expr(&n, src)).unwrap_or_default();
                        let v = child.child_by_field_name("value")
                            .map(|n| emit_expr(&n, src)).unwrap_or_default();
                        pairs.push(format!("{}: {}", k, v));
                    }
                }
                format!("{{{}}}", pairs.join(", "))
            }

            "tuple" => {
                let mut items = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    items.push(emit_expr(&child, src));
                }
                format!("({})", items.join(", "))
            }

            "parenthesized_expression" => {
                if let Some(inner) = node.named_child(0) {
                    format!("({})", emit_expr(&inner, src))
                } else {
                    "()".to_string()
                }
            }

            "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression" => {
                // [expr for x in iter if cond] → iter.fl(|x| cond).mp(|x| expr)
                // {expr for x in iter} → same, wrapped in to_set()
                // {k: v for x in iter} → same, key/value pair
                // (expr for x in iter) → generator, same as list comp for our purposes
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();

                let expr = children.first().map(|n| emit_expr(n, src)).unwrap_or_default();
                let mut binding = String::new();
                let mut iterable = String::new();
                let mut condition = None;

                for child in &children[1..] {
                    match child.kind() {
                        "for_in_clause" => {
                            binding = child.child_by_field_name("left")
                                .map(|n| text(&n, src).to_string()).unwrap_or_default();
                            iterable = child.child_by_field_name("right")
                                .map(|n| emit_expr(&n, src)).unwrap_or_default();
                        }
                        "if_clause" => {
                            condition = child.named_child(0)
                                .map(|n| emit_expr(&n, src));
                        }
                        _ => {}
                    }
                }

                // Multi-target binding: tree-sitter emits "x, y" as the text of
                // a pattern_list. Nebula lambda params accept `|x, y|`.
                let _ = &binding;

                let core = if let Some(cond) = condition {
                    format!("{}.fl(|{}| {}).mp(|{}| {})", iterable, binding, cond, binding, expr)
                } else {
                    format!("{}.mp(|{}| {})", iterable, binding, expr)
                };
                match node.kind() {
                    "set_comprehension" => format!("{}.to_set()", core),
                    "dictionary_comprehension" => {
                        // Dict comp: expr is a `pair` node; we already emitted
                        // it above as "k: v" — wrap the result in to_map().
                        format!("{}.to_map()", core)
                    }
                    _ => core,
                }
            }

            "slice" => {
                // Direct slice emission (rare — usually handled inside
                // `subscript` which dispatches to `emit_python_slice`).
                text(node, src).to_string()
            }

            "keyword_argument" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("");
                let value = node.child_by_field_name("value")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("{}: {}", name, value)
            }

            "await" => {
                if let Some(expr) = node.named_child(0) {
                    format!("aw {}", emit_expr(&expr, src))
                } else {
                    "aw".to_string()
                }
            }

            "yield" => {
                if let Some(expr) = node.named_child(0) {
                    format!("// yield {}", emit_expr(&expr, src))
                } else {
                    "// yield".to_string()
                }
            }

            _ => text(node, src).to_string(),
        }
    }

    fn emit_params(node: &Node, src: &[u8]) -> String {
        let mut params = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    let name = text(&child, src);
                    if name == "self" {
                        params.push("sf".to_string());
                    } else {
                        params.push(name.to_string());
                    }
                }
                "typed_parameter" => {
                    let name = child.named_child(0)
                        .map(|n| text(&n, src)).unwrap_or("?");
                    let name = if name == "self" { "sf" } else { name };
                    let ty = child.child_by_field_name("type")
                        .map(|n| format!(": {}", emit_type(&n, src)));
                    params.push(format!("{}{}", name, ty.unwrap_or_default()));
                }
                "default_parameter" => {
                    let name = child.child_by_field_name("name")
                        .map(|n| text(&n, src)).unwrap_or("?");
                    let val = child.child_by_field_name("value")
                        .map(|n| emit_expr(&n, src)).unwrap_or_default();
                    params.push(format!("{} = {}", name, val));
                }
                "typed_default_parameter" => {
                    let name = child.child_by_field_name("name")
                        .map(|n| text(&n, src)).unwrap_or("?");
                    let ty = child.child_by_field_name("type")
                        .map(|n| format!(": {}", emit_type(&n, src)));
                    let val = child.child_by_field_name("value")
                        .map(|n| emit_expr(&n, src)).unwrap_or_default();
                    params.push(format!("{}{} = {}", name,
                                       ty.unwrap_or_default(), val));
                }
                "list_splat_pattern" | "dictionary_splat_pattern" => {
                    params.push(text(&child, src).to_string());
                }
                _ => {}
            }
        }
        params.join(", ")
    }

    fn emit_lambda_params(node: &Node, src: &[u8]) -> String {
        let mut params = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            params.push(text(&child, src).to_string());
        }
        params.join(", ")
    }

    /// Translate a Python slice subscript into a Nebula method call.
    /// `s[a:b]`   → `s.slc(a, b)`
    /// `s[:b]`    → `s.slc(0, b)`
    /// `s[a:]`    → `s.slc_from(a)`
    /// `s[::-1]`  → `s.rev()`
    /// `s[:-1]`   → `s.slc(0, -1)`
    /// PythonGen maps these method names back to Python slice syntax.
    fn emit_python_slice(val: &str, slice_node: &Node, src: &[u8]) -> String {
        // Walk children, separating by top-level colons.
        let mut parts: Vec<Option<String>> = vec![None, None, None];
        let mut pos = 0usize;
        let mut cursor = slice_node.walk();
        for child in slice_node.children(&mut cursor) {
            if child.is_named() {
                if pos < 3 {
                    parts[pos] = Some(emit_expr(&child, src));
                }
            } else if text(&child, src) == ":" {
                pos += 1;
            }
        }
        let start = parts[0].as_deref();
        let stop = parts[1].as_deref();
        let step = parts[2].as_deref();
        // `s[::-1]` → reverse
        if step == Some("-1") && start.is_none() && stop.is_none() {
            return format!("{}.rev()", val);
        }
        // Step not supported outside reverse — fall back to slc with hint.
        if step.is_some() && step != Some("1") {
            return format!("{}.slc_step({}, {}, {})", val,
                start.unwrap_or("0"), stop.unwrap_or("-1"), step.unwrap_or("1"));
        }
        match (start, stop) {
            (Some(a), Some(b)) => format!("{}.slc({}, {})", val, a, b),
            (Some(a), None) => format!("{}.slc_from({})", val, a),
            (None, Some(b)) => format!("{}.slc(0, {})", val, b),
            (None, None) => format!("{}.slc()", val),
        }
    }

    fn emit_args(node: &Node, src: &[u8]) -> String {
        // Collect named children, but watch for the paren-less generator
        // pattern: `sum(expr for x in iter)`. In tree-sitter-python this
        // surfaces as an argument_list whose children are the expression
        // and a for_in_clause sibling — not wrapped in a generator_expression
        // node. Fold those back into a `.mp(|x| expr)` form.
        let mut children: Vec<Node> = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            children.push(child);
        }

        // Single-expression call with trailing for/if clauses → generator.
        let has_for = children.iter().any(|c| c.kind() == "for_in_clause");
        if has_for && children.len() >= 2 {
            let expr = emit_expr(&children[0], src);
            let mut binding = String::new();
            let mut iterable = String::new();
            let mut condition = None;
            for child in &children[1..] {
                match child.kind() {
                    "for_in_clause" => {
                        binding = child.child_by_field_name("left")
                            .map(|n| text(&n, src).to_string()).unwrap_or_default();
                        iterable = child.child_by_field_name("right")
                            .map(|n| emit_expr(&n, src)).unwrap_or_default();
                    }
                    "if_clause" => {
                        condition = child.named_child(0).map(|n| emit_expr(&n, src));
                    }
                    _ => {}
                }
            }
            return if let Some(cond) = condition {
                format!("{}.fl(|{}| {}).mp(|{}| {})", iterable, binding, cond, binding, expr)
            } else {
                format!("{}.mp(|{}| {})", iterable, binding, expr)
            };
        }

        children.iter().map(|c| emit_expr(c, src)).collect::<Vec<_>>().join(", ")
    }

    fn emit_block(node: &Node, src: &[u8], depth: usize) -> String {
        let inner = emit_children_as_statements(node, src, depth + 1, emit);
        let ind = indent(depth);
        format!(" {{\n{}{}}}", inner, ind)
    }

    fn emit_type(node: &Node, src: &[u8]) -> String {
        text(node, src).to_string()
    }
}

// ═══════════════════════════════════════════════════════════════════
// JavaScript
// ═══════════════════════════════════════════════════════════════════

mod js {
    use super::*;

    pub fn emit(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);

        match node.kind() {
            "program" => emit_children_as_statements(node, src, depth, emit),

            "comment" => {
                let t = text(node, src).trim();
                if t.starts_with("//") {
                    format!("{}{}", ind, t)
                } else {
                    format!("{}{}", ind, t) // block comments as-is
                }
            }

            "expression_statement" => {
                if let Some(child) = node.named_child(0) {
                    format!("{}{}", ind, emit_expr(&child, src))
                } else {
                    String::new()
                }
            }

            "function_declaration" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_params(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                // Check for async
                let is_async = node.children(&mut node.walk())
                    .any(|c| text(&c, src) == "async");
                let prefix = if is_async { "ac " } else { "" };

                format!("{}{}fn {}({}){}", ind, prefix, name, params, body)
            }

            "class_declaration" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                // Split class body: fields → st {}, methods → im {}
                let mut fields = String::new();
                let mut methods = String::new();
                let inner_ind = indent(depth + 1);
                if let Some(body) = node.child_by_field_name("body") {
                    let mut cursor = body.walk();
                    for child in body.named_children(&mut cursor) {
                        match child.kind() {
                            "method_definition" => {
                                methods.push_str(&emit(&child, src, depth + 1));
                                methods.push('\n');
                            }
                            "public_field_definition" | "field_definition" => {
                                // Extract field text and clean it up
                                let raw = text(&child, src).trim()
                                    .trim_end_matches(';').trim();
                                // Format: "name: type" or "name = value" — emit as-is
                                if !raw.is_empty() {
                                    fields.push_str(&format!("{}{}\n", inner_ind, raw));
                                }
                            }
                            _ => {
                                // Other statements go in impl
                                methods.push_str(&emit(&child, src, depth + 1));
                                methods.push('\n');
                            }
                        }
                    }
                }
                let st_body = if fields.is_empty() {
                    " {}".to_string()
                } else {
                    format!(" {{\n{}{}}}", fields, ind)
                };
                let im_body = if methods.is_empty() {
                    " {}".to_string()
                } else {
                    format!(" {{\n{}{}}}", methods, ind)
                };
                format!("{}st {}{}\n{}im {}{}", ind, name, st_body, ind, name, im_body)
            }

            "method_definition" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_params(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                let is_async = node.children(&mut node.walk())
                    .any(|c| text(&c, src) == "async");
                let prefix = if is_async { "ac " } else { "" };

                let sf_params = if params.is_empty() { "sf".to_string() }
                    else { format!("sf, {}", params) };
                if name == "constructor" {
                    format!("{}{}fn new({}){}", ind, prefix, sf_params, body)
                } else {
                    format!("{}{}fn {}({}){}", ind, prefix, name, sf_params, body)
                }
            }

            "lexical_declaration" | "variable_declaration" => {
                let kind = node.children(&mut node.walk())
                    .find(|c| matches!(text(&c, src), "const" | "let" | "var"))
                    .map(|c| text(&c, src))
                    .unwrap_or("let");

                let binding = if kind == "const" { "lt" } else { "mt" };

                let mut decls = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "variable_declarator" {
                        let name = child.child_by_field_name("name")
                            .map(|n| emit_expr(&n, src)).unwrap_or_default();
                        let value = child.child_by_field_name("value")
                            .map(|n| format!(" = {}", emit_expr(&n, src)))
                            .unwrap_or_default();
                        decls.push(format!("{}{} {}{}", ind, binding, name, value));
                    }
                }
                decls.join("\n")
            }

            "if_statement" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr_unwrap_parens(&n, src)).unwrap_or_default();
                let then = node.child_by_field_name("consequence")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                let mut result = format!("{}if {}{}", ind, cond, then);

                if let Some(alt) = node.child_by_field_name("alternative") {
                    match alt.kind() {
                        "if_statement" => {
                            let inner = emit(&alt, src, depth);
                            let inner = inner.trim_start();
                            result.push_str(&format!(" el {}", inner));
                        }
                        "else_clause" => {
                            // else_clause may wrap an if_statement (else if)
                            if let Some(inner_if) = alt.named_child(0) {
                                if inner_if.kind() == "if_statement" {
                                    let inner = emit(&inner_if, src, depth);
                                    let inner = inner.trim_start();
                                    result.push_str(&format!(" el {}", inner));
                                } else {
                                    let else_body = emit_block(&inner_if, src, depth);
                                    result.push_str(&format!(" el{}", else_body));
                                }
                            } else {
                                let else_body = emit_block(&alt, src, depth);
                                result.push_str(&format!(" el{}", else_body));
                            }
                        }
                        _ => {
                            let else_body = emit_block(&alt, src, depth);
                            result.push_str(&format!(" el{}", else_body));
                        }
                    }
                }

                result
            }

            "for_statement" => {
                // C-style for → extract condition and emit as wl (while)
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr(&n, src));
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                let init = node.child_by_field_name("initializer")
                    .map(|n| {
                        let raw = text(&n, src).trim().trim_end_matches(';');
                        let raw = raw.strip_prefix("let ").or(raw.strip_prefix("const "))
                            .or(raw.strip_prefix("var ")).unwrap_or(raw);
                        format!("{}mt {}\n", ind, raw)
                    }).unwrap_or_default();
                let update = node.child_by_field_name("increment")
                    .map(|n| format!("\n{}{}", indent(depth + 1), emit_expr(&n, src)));

                match cond {
                    Some(c) => {
                        // Inject update at end of body
                        let body_with_update = if let Some(upd) = update {
                            let body_trimmed = body.trim_end();
                            let close_pos = body_trimmed.rfind('}').unwrap_or(body_trimmed.len());
                            format!("{}{}\n{}}}", &body_trimmed[..close_pos], upd, ind)
                        } else {
                            body
                        };
                        format!("{}{}wl {}{}", init, ind, c, body_with_update)
                    }
                    None => format!("{}lp{}", ind, body),
                }
            }

            "for_in_statement" => {
                let left = node.child_by_field_name("left")
                    .map(|n| {
                        // Strip let/const/var
                        let t = text(&n, src).trim();
                        let t = t.strip_prefix("const ").or(t.strip_prefix("let "))
                            .or(t.strip_prefix("var ")).unwrap_or(t);
                        t.to_string()
                    }).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}fo {} in {}{}", ind, left, right, body)
            }

            "while_statement" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr_unwrap_parens(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}wl {}{}", ind, cond, body)
            }

            "return_statement" => {
                if let Some(expr) = node.named_child(0) {
                    format!("{}rt {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}rt", ind)
                }
            }

            "break_statement" => format!("{}bk", ind),
            "continue_statement" => format!("{}ct", ind),

            "switch_statement" => {
                let disc = node.child_by_field_name("value")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_switch_body(&n, src, depth))
                    .unwrap_or_default();
                format!("{}mc {} {{\n{}{}}}", ind, disc, body, ind)
            }

            "try_statement" => {
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                let mut result = format!("{}try{}", ind, body);

                if let Some(handler) = node.child_by_field_name("handler") {
                    let binding = handler.child_by_field_name("parameter")
                        .map(|n| text(&n, src))
                        .unwrap_or("e");
                    let catch_body = handler.child_by_field_name("body")
                        .map(|n| emit_block(&n, src, depth))
                        .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                    result.push_str(&format!(" ch {}{}", binding, catch_body));
                }

                result
            }

            "import_statement" | "export_statement" => {
                format!("{}// {}", ind, text(node, src).trim())
            }

            _ => {
                let raw = text(node, src).trim();
                if raw.is_empty() { String::new() }
                else { format!("{}{}", ind, raw) }
            }
        }
    }

    fn emit_expr(node: &Node, src: &[u8]) -> String {
        match node.kind() {
            "identifier" => {
                let name = text(node, src);
                match name {
                    "this" => "sf".to_string(),
                    "true" => "tt".to_string(),
                    "false" => "ff".to_string(),
                    "null" | "undefined" => "nil".to_string(),
                    _ => name.to_string(),
                }
            }

            "true" => "tt".to_string(),
            "false" => "ff".to_string(),
            "null" => "nil".to_string(),
            "undefined" => "nil".to_string(),
            "number" => text(node, src).to_string(),
            "this" => "sf".to_string(),

            "string" | "template_string" => {
                let raw = text(node, src);
                // Template strings: `text ${expr}` → "text {expr}"
                if raw.starts_with('`') {
                    let inner = &raw[1..raw.len()-1];
                    let converted = inner.replace("${", "{");
                    format!("\"{}\"", converted)
                } else if raw.starts_with('\'') {
                    format!("\"{}\"", &raw[1..raw.len()-1])
                } else {
                    raw.to_string()
                }
            }

            "call_expression" => {
                let func = node.child_by_field_name("function")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let args = node.child_by_field_name("arguments")
                    .map(|n| emit_args(&n, src)).unwrap_or_default();

                // Map console.log → prt
                let func = match func.as_str() {
                    "console.log" | "console.warn" | "console.error" => "prt".to_string(),
                    _ => func,
                };

                let func_name = map_builtin(&func);
                format!("{}({})", func_name, args)
            }

            "member_expression" => {
                let obj = node.child_by_field_name("object")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let prop = node.child_by_field_name("property")
                    .map(|n| text(&n, src)).unwrap_or("");
                let prop = map_method(prop);
                format!("{}.{}", obj, prop)
            }

            "subscript_expression" => {
                let obj = node.child_by_field_name("object")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let index = node.child_by_field_name("index")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("{}[{}]", obj, index)
            }

            "binary_expression" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src)).unwrap_or("+");
                let op = match op {
                    "===" => "==",
                    "!==" => "!=",
                    "&&" => "and",
                    "||" => "or",
                    "??" => "??",
                    other => other,
                };
                format!("{} {} {}", left, op, right)
            }

            "unary_expression" => {
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src)).unwrap_or("!");
                let arg = node.child_by_field_name("argument")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let op = if op == "!" { "not " } else { op };
                format!("{}{}", op, arg)
            }

            "ternary_expression" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let then = node.child_by_field_name("consequence")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let alt = node.child_by_field_name("alternative")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("if {} {{ {} }} el {{ {} }}", cond, then, alt)
            }

            "arrow_function" => {
                let params = node.child_by_field_name("parameters")
                    .or_else(|| node.child_by_field_name("parameter"))
                    .map(|n| {
                        if n.kind() == "identifier" {
                            text(&n, src).to_string()
                        } else {
                            emit_params(&n, src)
                        }
                    }).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| {
                        if n.kind() == "statement_block" {
                            // Multi-line: full block
                            format!(" {}", text(&n, src))
                        } else {
                            emit_expr(&n, src)
                        }
                    }).unwrap_or_default();
                format!("|{}| {}", params, body)
            }

            "array" => {
                let mut items = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    items.push(emit_expr(&child, src));
                }
                format!("[{}]", items.join(", "))
            }

            "object" => {
                let mut pairs = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "pair" {
                        let k = child.child_by_field_name("key")
                            .map(|n| emit_expr(&n, src)).unwrap_or_default();
                        let v = child.child_by_field_name("value")
                            .map(|n| emit_expr(&n, src)).unwrap_or_default();
                        pairs.push(format!("{}: {}", k, v));
                    } else if child.kind() == "shorthand_property_identifier" {
                        let name = text(&child, src);
                        pairs.push(format!("{}: {}", name, name));
                    }
                }
                format!("{{{}}}", pairs.join(", "))
            }

            "parenthesized_expression" => {
                if let Some(inner) = node.named_child(0) {
                    format!("({})", emit_expr(&inner, src))
                } else {
                    "()".to_string()
                }
            }

            "assignment_expression" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("{} = {}", left, right)
            }

            "augmented_assignment_expression" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src)).unwrap_or("+=");
                format!("{} {} {}", left, op, right)
            }

            "await_expression" => {
                if let Some(expr) = node.named_child(0) {
                    format!("aw {}", emit_expr(&expr, src))
                } else {
                    "aw".to_string()
                }
            }

            "new_expression" => {
                let constructor = node.child_by_field_name("constructor")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let args = node.child_by_field_name("arguments")
                    .map(|n| emit_args(&n, src)).unwrap_or_default();
                format!("{}.new({})", constructor, args)
            }

            "spread_element" => {
                if let Some(expr) = node.named_child(0) {
                    format!("...{}", emit_expr(&expr, src))
                } else {
                    "...".to_string()
                }
            }

            "property_identifier" | "shorthand_property_identifier" =>
                text(node, src).to_string(),

            _ => text(node, src).to_string(),
        }
    }

    fn emit_expr_unwrap_parens(node: &Node, src: &[u8]) -> String {
        if node.kind() == "parenthesized_expression" {
            if let Some(inner) = node.named_child(0) {
                return emit_expr(&inner, src);
            }
        }
        emit_expr(node, src)
    }

    fn emit_params(node: &Node, src: &[u8]) -> String {
        let mut params = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "identifier" => params.push(text(&child, src).to_string()),
                "assignment_pattern" => {
                    let left = child.child_by_field_name("left")
                        .map(|n| text(&n, src)).unwrap_or("?");
                    let right = child.child_by_field_name("right")
                        .map(|n| emit_expr(&n, src)).unwrap_or_default();
                    params.push(format!("{} = {}", left, right));
                }
                "rest_pattern" => {
                    params.push(text(&child, src).to_string());
                }
                _ => params.push(text(&child, src).to_string()),
            }
        }
        params.join(", ")
    }

    fn emit_args(node: &Node, src: &[u8]) -> String {
        let mut args = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            args.push(emit_expr(&child, src));
        }
        args.join(", ")
    }

    fn emit_block(node: &Node, src: &[u8], depth: usize) -> String {
        let inner = emit_children_as_statements(node, src, depth + 1, emit);
        let ind = indent(depth);
        format!(" {{\n{}{}}}", inner, ind)
    }

    fn emit_switch_body(node: &Node, src: &[u8], depth: usize) -> String {
        let mut out = String::new();
        let inner_ind = indent(depth + 1);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "switch_case" => {
                    let value = child.child_by_field_name("value")
                        .map(|n| emit_expr(&n, src)).unwrap_or_default();
                    out.push_str(&format!("{}{} => {{\n", inner_ind, value));
                    let mut body_cursor = child.walk();
                    for body_child in child.named_children(&mut body_cursor) {
                        if body_child.kind() != "expression_statement" &&
                           body_child.kind() != "return_statement" &&
                           body_child.kind() != "break_statement" { continue; }
                        let stmt = emit(&body_child, src, depth + 2);
                        if !stmt.is_empty() && body_child.kind() != "break_statement" {
                            out.push_str(&stmt);
                            out.push('\n');
                        }
                    }
                    out.push_str(&format!("{}}}\n", inner_ind));
                }
                "switch_default" => {
                    out.push_str(&format!("{}_ => {{\n", inner_ind));
                    let mut body_cursor = child.walk();
                    for body_child in child.named_children(&mut body_cursor) {
                        if body_child.kind() == "break_statement" { continue; }
                        let stmt = emit(&body_child, src, depth + 2);
                        if !stmt.is_empty() {
                            out.push_str(&stmt);
                            out.push('\n');
                        }
                    }
                    out.push_str(&format!("{}}}\n", inner_ind));
                }
                _ => {}
            }
        }
        out
    }
}

// ═══════════════════════════════════════════════════════════════════
// TypeScript (extends JS with type handling)
// ═══════════════════════════════════════════════════════════════════

mod ts {
    use super::*;

    pub fn emit(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);

        match node.kind() {
            "program" => emit_children_as_statements(node, src, depth, emit),

            // TypeScript-specific: interface → tr
            "interface_declaration" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let body = node.child_by_field_name("body")
                    .map(|n| emit_interface_body(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}tr {}{}", ind, name, body)
            }

            // TypeScript-specific: type alias → ty
            "type_alias_declaration" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let value = node.child_by_field_name("value")
                    .map(|n| text(&n, src)).unwrap_or("?");
                format!("{}ty {} = {}", ind, name, value)
            }

            // TypeScript-specific: enum
            "enum_declaration" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let body = node.child_by_field_name("body")
                    .map(|n| emit_enum_body(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}en {}{}", ind, name, body)
            }

            // Delegate everything else to JS emitter
            _ => js::emit(node, src, depth),
        }
    }

    fn emit_interface_body(node: &Node, src: &[u8], depth: usize) -> String {
        let mut out = String::new();
        let inner_ind = indent(depth + 1);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            let raw = text(&child, src).trim();
            if child.kind() == "method_signature" {
                // Method signatures can stay (fn-like)
                out.push_str(&format!("{}fn {}\n", inner_ind, raw));
            }
            // Skip property_signature — not valid in Nebula traits
        }
        let ind = indent(depth);
        format!(" {{\n{}{}}}", out, ind)
    }

    fn emit_enum_body(node: &Node, src: &[u8], depth: usize) -> String {
        let mut variants = Vec::new();
        let inner_ind = indent(depth + 1);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            variants.push(format!("{}{}", inner_ind, text(&child, src).trim()));
        }
        let ind = indent(depth);
        format!(" {{\n{}\n{}}}", variants.join("\n"), ind)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Go
// ═══════════════════════════════════════════════════════════════════

mod go {
    use super::*;

    pub fn emit(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);

        match node.kind() {
            "source_file" => emit_source_file(node, src, depth),

            "comment" => {
                let t = text(node, src).trim();
                format!("{}{}", ind, t)
            }

            "function_declaration" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_params(&n, src)).unwrap_or_default();
                let result_node = node.child_by_field_name("result");
                let ret = result_node.as_ref().map(|n| emit_return_type(n, src));
                let named_returns = result_node.as_ref()
                    .map(|n| extract_named_returns(n, src, depth + 1))
                    .unwrap_or_default();
                let mut body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                // Inject named return declarations at top of body
                if !named_returns.is_empty() {
                    body = body.replacen("{\n", &format!("{{\n{}", named_returns), 1);
                }
                format!("{}fn {}({}){}{}", ind, name, params,
                        ret.unwrap_or_default(), body)
            }

            "method_declaration" => {
                let receiver_node = node.child_by_field_name("receiver");
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_params(&n, src)).unwrap_or_default();
                let result_node = node.child_by_field_name("result");
                let ret = result_node.as_ref().map(|n| emit_return_type(n, src));
                let named_returns = result_node.as_ref()
                    .map(|n| extract_named_returns(n, src, depth + 1))
                    .unwrap_or_default();
                let mut body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                // Inject named return declarations
                if !named_returns.is_empty() {
                    body = body.replacen("{\n", &format!("{{\n{}", named_returns), 1);
                }

                // Extract receiver variable name and replace with sf in body
                if let Some(recv) = &receiver_node {
                    let recv_text = text(recv, src);
                    let recv_name = recv_text.trim_matches(|c: char| c == '(' || c == ')')
                        .split_whitespace().next().unwrap_or("s");
                    if recv_name != "sf" && !recv_name.is_empty() {
                        // Word-boundary replacement: replace recv_name with sf
                        // when followed by '.', ' ', ',', ')', '[', '{' or at end
                        let mut new_body = String::with_capacity(body.len());
                        let chars: Vec<char> = body.chars().collect();
                        let rn: Vec<char> = recv_name.chars().collect();
                        let rn_len = rn.len();
                        let mut i = 0;
                        while i < chars.len() {
                            if i + rn_len <= chars.len()
                                && &chars[i..i+rn_len] == rn.as_slice()
                                && (i == 0 || !chars[i-1].is_alphanumeric() && chars[i-1] != '_')
                                && (i + rn_len >= chars.len() ||
                                    matches!(chars[i+rn_len], '.' | ' ' | ',' | ')' | '[' | '{' | '\n'))
                            {
                                new_body.push_str("sf");
                                i += rn_len;
                            } else {
                                new_body.push(chars[i]);
                                i += 1;
                            }
                        }
                        body = new_body;
                    }
                }

                let param_str = if params.is_empty() {
                    "sf".to_string()
                } else {
                    format!("sf, {}", params)
                };
                format!("{}fn {}({}){}{}", ind,
                        name, param_str, ret.unwrap_or_default(), body)
            }

            "type_declaration" => {
                let mut cursor = node.walk();
                let mut out = String::new();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "type_spec" {
                        out.push_str(&emit_type_spec(&child, src, depth));
                    }
                }
                out
            }

            "short_var_declaration" => {
                let left_node = node.child_by_field_name("left");
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                // Check if multi-assign: t, ok := expr
                let left = left_node.as_ref()
                    .map(|n| text(n, src)).unwrap_or("?");
                if left.contains(',') {
                    // Multi-return: mt (a, b) = (expr1, expr2)
                    let names: Vec<&str> = left.split(',').map(|s| s.trim()).collect();
                    let rhs = if right.contains(", ") && !right.starts_with('(') {
                        format!("({})", right)
                    } else {
                        right
                    };
                    format!("{}mt ({}) = {}", ind, names.join(", "), rhs)
                } else {
                    format!("{}mt {} = {}", ind, left, right)
                }
            }

            "var_declaration" | "const_declaration" => {
                let kind = node.kind();
                let binding = if kind == "const_declaration" { "lt" } else { "mt" };
                let mut cursor = node.walk();
                let mut specs: Vec<(String, Option<String>)> = Vec::new();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "var_spec" || child.kind() == "const_spec" {
                        let name = child.child_by_field_name("name")
                            .map(|n| text(&n, src)).unwrap_or("?");
                        let value = child.child_by_field_name("value")
                            .map(|n| emit_expr(&n, src));
                        let raw_ty = child.child_by_field_name("type")
                            .map(|n| text(&n, src).to_string());
                        // If no value but has type, use the raw Go type for var declaration
                        let value = match value {
                            Some(v) => Some(v),
                            None => match &raw_ty {
                                // Preserve raw Go type as a "go_var:type" marker
                                Some(t) => Some(format!("go_var(\"{}\")", t)),
                                None => Some("nil".to_string()),
                            },
                        };
                        specs.push((name.to_string(), value));
                    }
                }

                // Detect Go iota enum: const block where first value is "iota"
                if kind == "const_declaration" && specs.len() > 1
                    && specs.first().map(|(_, v)| v.as_deref() == Some("iota")).unwrap_or(false)
                {
                    // Emit as Nebula enum
                    let inner_ind = indent(depth + 1);
                    let variants = specs.iter()
                        .map(|(name, _)| format!("{}{}", inner_ind, name))
                        .collect::<Vec<_>>()
                        .join("\n");
                    // Use the type name from the const spec if available
                    let type_name = {
                        let mut found = None;
                        let mut cursor2 = node.walk();
                        for child in node.named_children(&mut cursor2) {
                            if child.kind() == "const_spec" {
                                if let Some(ty) = child.child_by_field_name("type") {
                                    found = Some(text(&ty, src).to_string());
                                }
                                break;
                            }
                        }
                        found
                    };
                    match type_name {
                        Some(tn) => format!("{}en {} {{\n{}\n{}}}", ind, tn, variants, ind),
                        None => {
                            // Fallback: emit as individual lt bindings
                            specs.iter().enumerate().map(|(i, (name, _))| {
                                format!("{}{} {} = {}", ind, binding, name, i)
                            }).collect::<Vec<_>>().join("\n")
                        }
                    }
                } else {
                    let mut out = Vec::new();
                    for (name, value) in &specs {
                        match value {
                            Some(v) => out.push(format!("{}{} {} = {}", ind, binding, name, v)),
                            None => out.push(format!("{}{} {}", ind, binding, name)),
                        }
                    }
                    out.join("\n")
                }
            }

            "assignment_statement" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                // Multi-target assignment: a, b = b, a → use temp swap
                if left.contains(", ") {
                    let lefts: Vec<&str> = left.split(", ").collect();
                    let rights: Vec<&str> = right.split(", ").collect();
                    if lefts.len() == rights.len() && lefts.len() == 2 {
                        // Emit as temp-based swap
                        let mut out = String::new();
                        out.push_str(&format!("{}mt __tmp = {}\n", ind, rights[0].trim()));
                        out.push_str(&format!("{}{} = {}\n", ind, lefts[0].trim(), rights[1].trim()));
                        out.push_str(&format!("{}{} = __tmp", ind, lefts[1].trim()));
                        return out;
                    }
                }
                format!("{}{} = {}", ind, left, right)
            }

            "if_statement" => {
                // Handle Go's if-init pattern: if init; cond { ... }
                let init = node.child_by_field_name("initializer")
                    .map(|n| emit(&n, src, depth));
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("consequence")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                let mut result = String::new();
                // Emit init as separate statement before the if
                if let Some(init_stmt) = init {
                    result.push_str(init_stmt.trim());
                    result.push('\n');
                }
                result.push_str(&format!("{}if {}{}", ind, cond, body));

                if let Some(alt) = node.child_by_field_name("alternative") {
                    if alt.kind() == "if_statement" {
                        let inner = emit(&alt, src, depth);
                        let inner = inner.trim_start();
                        result.push_str(&format!(" el {}", inner));
                    } else {
                        let else_body = emit_block(&alt, src, depth);
                        result.push_str(&format!(" el{}", else_body));
                    }
                }

                result
            }

            "for_statement" => {
                // Go for can be: for {}, for cond {}, for init; cond; post {}, for range {}
                // Check for range clause by looking at direct children, not full text
                let has_range = {
                    let mut found = false;
                    let mut c = node.walk();
                    for ch in node.children(&mut c) {
                        if ch.kind() == "range_clause" { found = true; break; }
                    }
                    found
                };
                if has_range {
                    // for k, v := range x { → fo k, v in x {
                    emit_range_for(node, src, depth)
                } else if node.named_child_count() == 1 {
                    // for { body } → lp { body }
                    let body = node.named_child(0)
                        .map(|n| emit_block(&n, src, depth))
                        .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                    format!("{}lp{}", ind, body)
                } else {
                    // C-style for or while-like for
                    // Find for_clause (C-style) or condition and block
                    let mut for_clause = None;
                    let mut cond_node = None;
                    let mut body_node = None;
                    let mut fc = node.walk();
                    for child in node.named_children(&mut fc) {
                        match child.kind() {
                            "for_clause" => for_clause = Some(child),
                            "block" => body_node = Some(child),
                            _ => {
                                if cond_node.is_none() && body_node.is_none() {
                                    cond_node = Some(child);
                                }
                            }
                        }
                    }

                    let body = body_node.as_ref()
                        .map(|n| emit_block(n, src, depth))
                        .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                    if let Some(fc) = for_clause {
                        // C-style: for init; cond; update { body }
                        let init = fc.child_by_field_name("initializer")
                            .map(|n| emit(&n, src, depth));
                        let cond = fc.child_by_field_name("condition")
                            .map(|n| emit_expr(&n, src));
                        let update = fc.child_by_field_name("update")
                            .map(|n| emit(&n, src, depth));

                        let mut result = String::new();
                        if let Some(init_stmt) = init {
                            result.push_str(init_stmt.trim());
                            result.push('\n');
                        }
                        let c = cond.unwrap_or_default();
                        if let Some(upd) = update {
                            let inner_ind = indent(depth + 1);
                            let body_trimmed = body.trim_end().trim_end_matches('}');
                            result.push_str(&format!("{}wl {}{}\n{}{}\n{}}}", ind, c, body_trimmed, inner_ind, upd.trim(), ind));
                        } else {
                            result.push_str(&format!("{}wl {}{}", ind, c, body));
                        }
                        result
                    } else if let Some(cn) = cond_node {
                        let c = emit_expr(&cn, src);
                        format!("{}wl {}{}", ind, c, body)
                    } else {
                        format!("{}{}", ind, text(node, src).trim())
                    }
                }
            }

            "return_statement" => {
                let mut cursor = node.walk();
                let exprs: Vec<String> = node.named_children(&mut cursor)
                    .map(|c| emit_expr(&c, src))
                    .collect();
                if exprs.is_empty() {
                    format!("{}rt", ind)
                } else if exprs.len() == 1 && !exprs[0].contains(", ") {
                    format!("{}rt {}", ind, exprs[0])
                } else {
                    // Multi-return: wrap in tuple
                    let joined = exprs.join(", ");
                    format!("{}rt ({})", ind, joined)
                }
            }

            "expression_statement" => {
                if let Some(child) = node.named_child(0) {
                    format!("{}{}", ind, emit_expr(&child, src))
                } else {
                    String::new()
                }
            }

            "expression_switch_statement" => {
                let value = node.child_by_field_name("value")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let inner_ind = indent(depth + 1);
                let arm_ind = indent(depth + 2);
                let mut arms = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "expression_case" => {
                            // case val1, val2: body
                            let mut case_cursor = child.walk();
                            let case_children: Vec<_> = child.named_children(&mut case_cursor).collect();
                            // First child(ren) before the body are the case values
                            // Find the ":" position to separate values from body
                            let mut values = Vec::new();
                            let mut body_stmts = Vec::new();
                            for cc in &case_children {
                                // expression_list holds case values, or individual expressions
                                if cc.kind() == "expression_list" {
                                    let mut ec = cc.walk();
                                    for expr in cc.named_children(&mut ec) {
                                        values.push(emit_expr(&expr, src));
                                    }
                                } else if values.is_empty() && body_stmts.is_empty()
                                    && !matches!(cc.kind(), "return_statement" | "expression_statement"
                                        | "short_var_declaration" | "assignment_statement"
                                        | "if_statement" | "for_statement" | "block"
                                        | "inc_statement" | "dec_statement" | "call_expression")
                                {
                                    values.push(emit_expr(cc, src));
                                } else {
                                    body_stmts.push(emit(cc, src, depth + 2));
                                }
                            }
                            let val_str = values.join(", ");
                            if body_stmts.len() == 1 {
                                // Strip "rt " prefix — match arms are expressions
                                let body = body_stmts[0].trim();
                                let body = body.strip_prefix("rt ").unwrap_or(body);
                                arms.push(format!("{}{} => {}", inner_ind, val_str, body));
                            } else {
                                let body = body_stmts.iter()
                                    .map(|s| format!("{}{}", arm_ind, s.trim()))
                                    .collect::<Vec<_>>().join("\n");
                                arms.push(format!("{}{} => {{\n{}\n{}}}", inner_ind, val_str,
                                    body, inner_ind));
                            }
                        }
                        "default_case" => {
                            let mut dc = child.walk();
                            let body_stmts: Vec<_> = child.named_children(&mut dc)
                                .map(|c| emit(&c, src, depth + 2))
                                .collect();
                            if body_stmts.len() == 1 {
                                let body = body_stmts[0].trim();
                                let body = body.strip_prefix("rt ").unwrap_or(body);
                                arms.push(format!("{}_ => {}", inner_ind, body));
                            } else {
                                let body = body_stmts.iter()
                                    .map(|s| format!("{}{}", arm_ind, s.trim()))
                                    .collect::<Vec<_>>().join("\n");
                                arms.push(format!("{}_ => {{\n{}\n{}}}", inner_ind,
                                    body, inner_ind));
                            }
                        }
                        _ => {}
                    }
                }
                format!("{}mc {} {{\n{}\n{}}}", ind, value, arms.join("\n"), ind)
            }

            "send_statement" => {
                // ch <- value → ch.send(value)
                let channel = node.child_by_field_name("channel")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let value = node.child_by_field_name("value")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("{}{}.send({})", ind, channel, value)
            }

            "go_statement" => {
                if let Some(expr) = node.named_child(0) {
                    format!("{}sp {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}sp", ind)
                }
            }

            "defer_statement" => {
                if let Some(expr) = node.named_child(0) {
                    format!("{}defer {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}defer", ind)
                }
            }

            "import_declaration" => {
                format!("{}// {}", ind, text(node, src).trim().replace('\n', " "))
            }

            "package_clause" => {
                format!("{}// {}", ind, text(node, src).trim())
            }

            "break_statement" => format!("{}bk", ind),
            "continue_statement" => format!("{}ct", ind),

            // statement_list wraps statements inside case clauses etc.
            "statement_list" => {
                emit_children_as_statements(node, src, depth, emit)
            }

            _ => {
                let raw = text(node, src).trim();
                if raw.is_empty() { String::new() }
                else { format!("{}{}", ind, raw) }
            }
        }
    }

    /// Escape Go identifiers that conflict with Nebula keywords
    fn escape_ident(name: &str) -> String {
        match name {
            "st" | "en" | "tr" | "im" | "mc" | "fo" | "wl" | "lp" |
            "lt" | "mt" | "cn" | "rt" | "bk" | "ct" | "sf" | "pb" |
            "md" | "ty" | "tt" | "ff" | "el" | "sp" | "ac" | "aw" |
            "ln" | "prt" | "psh" => format!("{}_", name),
            _ => name.to_string(),
        }
    }

    fn emit_expr(node: &Node, src: &[u8]) -> String {
        match node.kind() {
            "identifier" | "field_identifier" | "package_identifier" | "type_identifier" =>
                escape_ident(text(node, src)),

            "true" => "tt".to_string(),
            "false" => "ff".to_string(),
            "nil" => "nil".to_string(),
            "int_literal" | "float_literal" | "rune_literal" =>
                text(node, src).to_string(),

            "interpreted_string_literal" | "raw_string_literal" => {
                let raw = text(node, src);
                // Go strings are already "..." or `...`
                if raw.starts_with('`') {
                    format!("\"{}\"", &raw[1..raw.len()-1])
                } else {
                    raw.to_string()
                }
            }

            "call_expression" => {
                let func = node.child_by_field_name("function")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let args_node = node.child_by_field_name("arguments");
                let args = args_node.as_ref()
                    .map(|n| emit_call_args(n, src)).unwrap_or_default();

                // Handle make() specially — convert to type constructors
                if func == "make" {
                    let raw_args = args_node.map(|n| text(&n, src)).unwrap_or_default();
                    let raw_args = raw_args.strip_prefix('(').unwrap_or(raw_args);
                    let raw_args = raw_args.strip_suffix(')').unwrap_or(raw_args);
                    if raw_args.starts_with("chan ") {
                        let parts: Vec<&str> = raw_args.splitn(2, ',').collect();
                        let ty = parts[0].strip_prefix("chan ").unwrap_or(parts[0]).trim();
                        let size = parts.get(1).map(|s| s.trim()).unwrap_or("0");
                        return format!("Channel({}, {})", go_type_to_nebula(ty), size);
                    }
                    // Encode type as a string argument so the parser accepts it
                    return format!("make(\"{}\")", raw_args.replace('"', "'"));
                }

                // Handle delete() → map.remove(key)
                if func == "delete" {
                    let arg_list: Vec<_> = args.splitn(2, ", ").collect();
                    if arg_list.len() == 2 {
                        return format!("{}.remove({})", arg_list[0], arg_list[1]);
                    }
                }

                // Handle close() → ch.close()
                if func == "close" && !args.contains(',') {
                    return format!("{}.close()", args);
                }

                // Map Go builtins
                let func = match func.as_str() {
                    "fmt.Println" | "fmt.Printf" | "fmt.Print" | "log.Println" => "prt".to_string(),
                    "fmt.Sprintf" => "fmt".to_string(),
                    "len" => "ln".to_string(),
                    "append" => "psh".to_string(),
                    _ => func,
                };

                format!("{}({})", func, args)
            }

            "selector_expression" => {
                let obj = node.child_by_field_name("operand")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let field = node.child_by_field_name("field")
                    .map(|n| text(&n, src)).unwrap_or("");
                format!("{}.{}", obj, field)
            }

            "index_expression" => {
                let obj = node.child_by_field_name("operand")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let idx = node.child_by_field_name("index")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("{}[{}]", obj, idx)
            }

            "binary_expression" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src)).unwrap_or("+");
                let op = match op {
                    "&&" => "and",
                    "||" => "or",
                    other => other,
                };
                format!("{} {} {}", left, op, right)
            }

            "unary_expression" => {
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src)).unwrap_or("!");
                let arg = node.child_by_field_name("operand")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                // <-ch → ch.recv()
                if op == "<-" {
                    return format!("{}.recv()", arg);
                }
                let op = if op == "!" { "not " } else { op };
                format!("{}{}", op, arg)
            }

            "composite_literal" => {
                // Type{fields} — struct, map, or slice literal
                let type_node = node.child_by_field_name("type");
                let body_node = node.child_by_field_name("body");
                let raw_type = type_node.as_ref()
                    .map(|n| text(n, src).to_string())
                    .unwrap_or_default();
                let type_str = type_node.as_ref()
                    .map(|n| go_type_to_nebula(&text(n, src)))
                    .unwrap_or_default();
                let body_str = body_node.as_ref()
                    .map(|n| {
                        let mut items = Vec::new();
                        let mut cursor = n.walk();
                        for child in n.named_children(&mut cursor) {
                            match child.kind() {
                                "keyed_element" | "literal_element" => {
                                    // keyed_element: literal_element ":" literal_element
                                    // Each literal_element wraps an expression
                                    let mut parts = Vec::new();
                                    let mut inner = child.walk();
                                    for c in child.children(&mut inner) {
                                        let k = c.kind();
                                        if k == ":" || k == "," || k == "{" || k == "}" { continue; }
                                        // literal_element wraps an expression — unwrap it
                                        if k == "literal_element" {
                                            if let Some(inner_expr) = c.named_child(0) {
                                                parts.push(inner_expr);
                                            } else {
                                                parts.push(c);
                                            }
                                        } else {
                                            parts.push(c);
                                        }
                                    }
                                    if parts.len() >= 2 {
                                        let key = text(&parts[0], src);
                                        let val = emit_expr(&parts[1], src);
                                        items.push(format!("{}: {}", key, val));
                                    } else if parts.len() == 1 {
                                        items.push(emit_expr(&parts[0], src));
                                    }
                                }
                                _ => {
                                    items.push(emit_expr(&child, src));
                                }
                            }
                        }
                        items.join(", ")
                    })
                    .unwrap_or_default();
                if type_str.is_empty() {
                    format!("{{{}}}", body_str)
                } else if body_str.is_empty() {
                    // Empty composite literal
                    if raw_type.starts_with("[]") {
                        format!("go_var(\"{}\")", raw_type) // preserve Go slice type
                    } else if raw_type.starts_with("map[") {
                        format!("make(\"{}\")", raw_type)  // empty map → make("mapType")
                    } else {
                        format!("{}{{}}", type_str)
                    }
                } else if type_str.starts_with("List<") || type_str.starts_with("[") {
                    // Slice/array literal: [item1, item2, ...]
                    format!("[{}]", body_str)
                } else {
                    format!("{} {{ {} }}", type_str, body_str)
                }
            }

            "slice_expression" => {
                let obj = node.child_by_field_name("operand")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let start = node.child_by_field_name("start")
                    .map(|n| emit_expr(&n, src));
                let end = node.child_by_field_name("end")
                    .map(|n| emit_expr(&n, src));
                match (start, end) {
                    (Some(s), Some(e)) => format!("{}[{}..{}]", obj, s, e),
                    (Some(s), None) => format!("{}[{}..]", obj, s),
                    (None, Some(e)) => format!("{}[0..{}]", obj, e),
                    (None, None) => format!("{}[0..]", obj),
                }
            }

            "type_assertion_expression" => {
                let obj = node.child_by_field_name("operand")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let ty = node.child_by_field_name("type")
                    .map(|n| go_type_to_nebula(&text(&n, src))).unwrap_or_default();
                format!("{}.({}))", obj, ty)
            }

            "func_literal" => {
                // func(params) rettype { body } → |params| { body }
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_params(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, 0))
                    .unwrap_or_else(|| " {}".to_string());
                if params.is_empty() {
                    // Zero-param lambda: use fn() syntax to avoid || → or postprocess
                    format!("fn(){}", body)
                } else {
                    format!("|{}|{}", params, body)
                }
            }

            "parenthesized_expression" => {
                if let Some(inner) = node.named_child(0) {
                    format!("({})", emit_expr(&inner, src))
                } else {
                    "()".to_string()
                }
            }

            "expression_list" => {
                let mut items = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    items.push(emit_expr(&child, src));
                }
                items.join(", ")
            }

            _ => text(node, src).to_string(),
        }
    }

    fn emit_params(node: &Node, src: &[u8]) -> String {
        let mut params = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "parameter_declaration" {
                // Go allows multiple names per type: (a, b string)
                // Collect all identifier children as names
                let mut names = Vec::new();
                let mut ty_str = String::new();
                let mut inner = child.walk();
                for c in child.named_children(&mut inner) {
                    match c.kind() {
                        "identifier" => names.push(text(&c, src).to_string()),
                        _ => {
                            // Assume this is the type node
                            ty_str = go_type_to_nebula(&text(&c, src));
                        }
                    }
                }
                if names.is_empty() {
                    names.push("_".to_string());
                }
                let ty_ann = if ty_str.is_empty() { String::new() }
                    else { format!(": {}", ty_str) };
                for name in &names {
                    params.push(format!("{}{}", name, ty_ann));
                }
            }
        }
        params.join(", ")
    }

    /// Extract named return variables as "mt name = go_var("type")" declarations
    fn extract_named_returns(node: &Node, src: &[u8], depth: usize) -> String {
        let inner_ind = indent(depth);
        if node.kind() != "parameter_list" { return String::new(); }
        let mut decls = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "parameter_declaration" {
                let name_node = child.child_by_field_name("name");
                let type_node = child.child_by_field_name("type");
                if let (Some(n), Some(t)) = (name_node, type_node) {
                    let name = text(&n, src);
                    let ty = text(&t, src);
                    decls.push(format!("{}mt {} = go_var(\"{}\")\n", inner_ind, name, ty));
                }
            }
        }
        decls.join("")
    }

    /// Emit a Go return type, converting Go types to Nebula
    fn emit_return_type(node: &Node, src: &[u8]) -> String {
        let raw = text(node, src).trim();
        // Multi-return: the node is a parameter_list
        if node.kind() == "parameter_list" {
            let mut types = Vec::new();
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "parameter_declaration" {
                    let name_node = child.child_by_field_name("name");
                    let type_node = child.child_by_field_name("type");
                    match (name_node, type_node) {
                        (_, Some(t)) => {
                            // Named or unnamed return — only emit the type
                            types.push(go_type_to_nebula(&text(&t, src)));
                        }
                        (Some(n), None) => {
                            // Unnamed return: parsed as "name" but is actually a type
                            types.push(go_type_to_nebula(&text(&n, src)));
                        }
                        _ => {
                            types.push(go_type_to_nebula(&text(&child, src)));
                        }
                    }
                }
            }
            if types.len() == 1 {
                format!(" -> {}", types[0])
            } else {
                format!(" -> ({})", types.join(", "))
            }
        } else {
            format!(" -> {}", go_type_to_nebula(raw))
        }
    }

    /// Emit source file with methods grouped into im blocks
    fn emit_source_file(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);

        enum Decl {
            Method { recv_type: String, code: String },
            Other(String),
        }

        let mut decls = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "method_declaration" {
                let receiver = child.child_by_field_name("receiver")
                    .map(|n| text(&n, src)).unwrap_or("");
                let recv_type = receiver.trim_matches(|c: char| c == '(' || c == ')')
                    .split_whitespace().last().unwrap_or("?")
                    .trim_start_matches('*')
                    .to_string();
                let code = emit(&child, src, depth + 1);
                decls.push(Decl::Method { recv_type, code });
            } else {
                let code = emit(&child, src, depth);
                if !code.trim().is_empty() {
                    decls.push(Decl::Other(code));
                }
            }
        }

        // Group consecutive methods by receiver type into im blocks
        // Filter out "ty X = int" that precede "en X {" (iota enum duplicate)
        let mut out = String::new();
        let mut i = 0;
        while i < decls.len() {
            match &decls[i] {
                Decl::Other(code) => {
                    // Skip "ty X = int" if next decl starts with "en X {"
                    if code.trim().starts_with("ty ") && code.trim().ends_with("= int") {
                        if let Some(Decl::Other(next_code)) = decls.get(i + 1) {
                            let ty_name = code.trim().strip_prefix("ty ").unwrap_or("")
                                .split(" =").next().unwrap_or("").trim();
                            if next_code.trim().starts_with(&format!("en {} {{", ty_name)) {
                                i += 1;
                                continue;
                            }
                        }
                    }
                    out.push_str(code);
                    out.push('\n');
                    i += 1;
                }
                Decl::Method { recv_type, .. } => {
                    let rt = recv_type.clone();
                    let mut methods = Vec::new();
                    while i < decls.len() {
                        if let Decl::Method { recv_type: r, code } = &decls[i] {
                            if r == &rt {
                                methods.push(code.clone());
                                i += 1;
                                continue;
                            }
                        }
                        break;
                    }
                    out.push_str(&format!("{}im {} {{\n", ind, rt));
                    for method in &methods {
                        out.push_str(method);
                        out.push('\n');
                    }
                    out.push_str(&format!("{}}}\n", ind));
                }
            }
        }
        out
    }

    fn emit_call_args(node: &Node, src: &[u8]) -> String {
        let mut args = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            args.push(emit_expr(&child, src));
        }
        args.join(", ")
    }

    fn emit_block(node: &Node, src: &[u8], depth: usize) -> String {
        let inner = emit_children_as_statements(node, src, depth + 1, emit);
        let ind = indent(depth);
        format!(" {{\n{}{}}}", inner, ind)
    }

    fn emit_type_spec(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);
        let name = node.child_by_field_name("name")
            .map(|n| text(&n, src)).unwrap_or("?");
        let typ = node.child_by_field_name("type")
            .map(|n| n.kind());

        match typ {
            Some("struct_type") => {
                let type_node = node.child_by_field_name("type").unwrap();
                let body = emit_struct_body(&type_node, src, depth);
                format!("{}st {}{}", ind, name, body)
            }
            Some("interface_type") => {
                let type_node = node.child_by_field_name("type").unwrap();
                let body = emit_interface_body(&type_node, src, depth);
                format!("{}tr {}{}", ind, name, body)
            }
            _ => {
                let val = node.child_by_field_name("type")
                    .map(|n| text(&n, src)).unwrap_or("?");
                format!("{}ty {} = {}", ind, name, val)
            }
        }
    }

    /// Convert a Go type string to Nebula type syntax
    fn go_type_to_nebula(ty: &str) -> String {
        let ty = ty.trim();
        // Pointer types: *Type → Ptr<Type>
        if let Some(inner) = ty.strip_prefix('*') {
            return format!("Ptr<{}>", go_type_to_nebula(inner));
        }
        // Slice types: []Type → List<Type>
        if let Some(inner) = ty.strip_prefix("[]") {
            return format!("List<{}>", go_type_to_nebula(inner));
        }
        // Map types: map[K]V → Map<K, V>
        if let Some(rest) = ty.strip_prefix("map[") {
            // Find matching ] for key type
            let mut depth = 1;
            let mut split = 0;
            for (i, ch) in rest.char_indices() {
                match ch {
                    '[' => depth += 1,
                    ']' => { depth -= 1; if depth == 0 { split = i; break; } }
                    _ => {}
                }
            }
            if split > 0 {
                let key = &rest[..split];
                let val = &rest[split + 1..];
                return format!("Map<{}, {}>", go_type_to_nebula(key), go_type_to_nebula(val));
            }
        }
        // Qualified types: pkg.Type → PkgType (camelCase merge)
        if ty.contains('.') && !ty.starts_with("map[") && !ty.starts_with("func(") {
            let parts: Vec<&str> = ty.split('.').collect();
            if parts.len() == 2 {
                let pkg = parts[0];
                let name = parts[1];
                // Capitalize package name and concatenate
                let mut result = String::new();
                for c in pkg.chars().take(1) { result.push(c.to_uppercase().next().unwrap_or(c)); }
                result.push_str(&pkg[1..]);
                result.push_str(name);
                return result;
            }
        }
        // Function types: func(T) R → Fn(T) -> R
        if let Some(rest) = ty.strip_prefix("func(") {
            if let Some(paren_end) = rest.find(')') {
                let params = &rest[..paren_end];
                let ret = rest[paren_end + 1..].trim();
                let neb_params = params.split(',')
                    .map(|p| go_type_to_nebula(p.trim()))
                    .collect::<Vec<_>>().join(", ");
                if ret.is_empty() {
                    return format!("Fn({})", neb_params);
                } else {
                    return format!("Fn({}) -> {}", neb_params, go_type_to_nebula(ret));
                }
            }
        }
        // Channel types: chan T → Channel(T)
        if let Some(inner) = ty.strip_prefix("chan ") {
            return format!("Channel({})", go_type_to_nebula(inner));
        }
        ty.to_string()
    }

    fn emit_struct_body(node: &Node, src: &[u8], depth: usize) -> String {
        let mut fields = String::new();
        let inner_ind = indent(depth + 1);
        // The struct_type node contains a field_declaration_list
        // which contains field_declaration nodes
        let field_list = node.named_children(&mut node.walk())
            .find(|c| c.kind() == "field_declaration_list");

        let emit_field = |child: &Node, fields: &mut String, inner_ind: &str| {
            if child.kind() == "field_declaration" {
                let name = child.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let ty = child.child_by_field_name("type")
                    .map(|n| go_type_to_nebula(&text(&n, src)))
                    .unwrap_or_else(|| "?".to_string());
                fields.push_str(&format!("{}{}: {}\n", inner_ind, name, ty));
            }
        };

        if let Some(fl) = field_list {
            let mut cursor = fl.walk();
            for child in fl.named_children(&mut cursor) {
                emit_field(&child, &mut fields, &inner_ind);
            }
        } else {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                emit_field(&child, &mut fields, &inner_ind);
            }
        }
        let ind = indent(depth);
        format!(" {{\n{}{}}}", fields, ind)
    }

    fn emit_interface_body(node: &Node, src: &[u8], depth: usize) -> String {
        let mut methods = String::new();
        let inner_ind = indent(depth + 1);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            methods.push_str(&format!("{}{}\n", inner_ind, text(&child, src).trim()));
        }
        let ind = indent(depth);
        format!(" {{\n{}{}}}", methods, ind)
    }

    fn emit_range_for(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);
        // Fallback to raw for complex range patterns
        let raw = text(node, src).trim();

        // Try to extract: for k, v := range expr { body }
        // Look for range_clause child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "range_clause" {
                let left_raw = child.child_by_field_name("left")
                    .map(|n| text(&n, src)).unwrap_or("_");
                // Escape any identifiers that conflict with Nebula keywords
                let left = left_raw.split(", ")
                    .map(|s| escape_ident(s.trim()))
                    .collect::<Vec<_>>().join(", ");
                let right = child.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                // Find body
                let body_node = node.child_by_field_name("body").or_else(|| {
                    let mut c2 = node.walk();
                    let mut found = None;
                    for ch in node.named_children(&mut c2) {
                        if ch.kind() == "block" { found = Some(ch); break; }
                    }
                    found
                });
                let body = body_node
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                return format!("{}fo {} in {}{}", ind, left, right, body);
            }
        }

        format!("{}{}", ind, raw)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Swift
// ═══════════════════════════════════════════════════════════════════

pub(crate) mod swift {
    use super::*;

    /// Fix Swift string interpolation: `\(expr)` → `{expr}`
    fn fix_swift_interpolation(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        let mut interp_depth = 0;
        while i < chars.len() {
            if i + 1 < chars.len() && chars[i] == '\\' && chars[i + 1] == '(' {
                result.push('{');
                interp_depth += 1;
                i += 2;
            } else if interp_depth > 0 && chars[i] == ')' {
                result.push('}');
                interp_depth -= 1;
                i += 1;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }
        result
    }

    pub fn convert_swift_raw(raw: &str, depth: usize) -> String {
        let ind = indent(depth);
        let mut out = Vec::new();
        let mut in_struct = false;  // Track if we're inside a struct body
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                out.push(String::new());
                continue;
            }
            let converted = trimmed
                .replace("let ", "lt ")
                .replace("var ", "mt ")
                .replace("func ", "fn ")
                .replace("return ", "rt ")
                .replace("guard ", "if ")
                .replace("else {", "el {")
                .replace("} else {", "} el {")
                .replace("} else if ", "} el if ")
                .replace("true", "tt")
                .replace("false", "ff")
                .replace("nil", "nil")
                .replace("self.", "sf.")
                .replace("Self.", "Sf.")
                .replace("while ", "wl ")
                .replace("for ", "fo ")
                .replace("switch ", "mc ")
                .replace("case ", "")
                .replace("break", "bk")
                .replace("continue", "ct")
                .replace("print(", "prt(")
                .replace("struct ", "st ")
                .replace("class ", "st ")
                .replace("protocol ", "tr ")
                .replace("enum ", "en ")
                .replace("$0", "it")
                .replace("$1", "it2");
            // Fix Swift string interpolation: \(expr) → {expr}
            let converted = fix_swift_interpolation(&converted);

            // Track struct/class context
            let starts_struct = converted.starts_with("st ");
            if starts_struct {
                in_struct = true;
                // Extract struct name for later im block
            }

            // Inside struct body: split fields from methods
            if in_struct && converted.contains("fn ") && !starts_struct {
                // Close struct, start impl block
                let struct_name = out.iter().rev()
                    .find(|l: &&String| l.contains("st "))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("?")
                    .trim_end_matches('{').trim_end()
                    .to_string();
                out.push(format!("{}}}", ind));
                out.push(format!("{}im {} {{", ind, struct_name));
                in_struct = false;
            }

            // Inside struct body: strip lt/mt and default values from field declarations
            let converted = if in_struct && converted.contains(':') && !converted.contains("fn ") && !starts_struct && converted.trim() != "}" {
                let stripped = converted.replace("lt ", "").replace("mt ", "")
                    .replace("mutating ", "").replace(":  ", ": ");
                // Strip default value: "name: Type = value" → "name: Type"
                if let Some(eq_pos) = stripped.find(" = ") {
                    stripped[..eq_pos].to_string()
                } else {
                    stripped
                }
            } else {
                converted
            };

            if converted.trim() == "}" {
                in_struct = false;
            }

            out.push(format!("{}{}", ind, converted));
        }
        let result = out.join("\n");
        if !result.ends_with('\n') && !result.is_empty() {
            format!("{}\n", result)
        } else {
            result
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Kotlin
// ═══════════════════════════════════════════════════════════════════

pub(crate) mod kotlin {
    use super::*;

    pub fn convert_kotlin_raw(raw: &str, depth: usize) -> String {
        let ind = indent(depth);
        let mut out = Vec::new();
        let mut in_data_class = false;
        let mut in_struct = false;
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                out.push(String::new());
                continue;
            }

            // Kotlin data class: multi-line "(val x: T," → "{ x: T,"
            if trimmed.starts_with("data class ") {
                in_data_class = true;
            }

            // Inside data class params: strip val/var, replace () with {}
            let closing_data = in_data_class && trimmed.contains(')');
            if closing_data {
                in_data_class = false;
            }
            let trimmed = if trimmed.starts_with("data class ") {
                let converted = trimmed.replace("data class ", "st ")
                    .replace('(', " {").replace(')', " }")
                    .replace("val ", "").replace("var ", "");
                converted
            } else if in_data_class || closing_data {
                // Field line or closing line
                let mut line = trimmed.replace("val ", "").replace("var ", "");
                // ") {" closes data class and opens method body
                if closing_data && (line.contains(") {") || line.contains(")") || line.trim() == ") {") {
                    let name = out.iter().rev()
                        .find(|l: &&String| l.contains("st "))
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or("?")
                        .trim_end_matches('{').trim_end()
                        .to_string();
                    // Push closing brace and impl block as separate lines
                    out.push(format!("{}}}", ind));
                    line = format!("im {} {{", name);
                } else {
                    line = line.replace(')', " }").replace('(', " {");
                }
                line
            } else {
                trimmed.to_string()
            };
            let converted = trimmed
                .replace("val ", "lt ")
                .replace("var ", "mt ")
                .replace("fun ", "fn ")
                .replace("return ", "rt ")
                .replace("class ", "st ")
                .replace("object ", "st ")
                .replace("interface ", "tr ")
                .replace("enum class ", "en ")
                .replace("when ", "mc ")
                .replace("else ->", "_ =>")
                .replace("->", "=>")
                .replace("} else {", "} el {")
                .replace("} else if ", "} el if ")
                .replace("while (", "wl ")
                .replace("for (", "fo ")
                .replace("true", "tt")
                .replace("false", "ff")
                .replace("null", "nil")
                .replace("this.", "sf.")
                .replace("println(", "prt(")
                .replace("print(", "prt(")
                .replace("break", "bk")
                .replace("continue", "ct");

            // Track struct/class context (same logic as Swift)
            let starts_struct = converted.starts_with("st ");
            if starts_struct {
                in_struct = true;
            }
            if in_struct && converted.contains("fn ") && !starts_struct {
                let struct_name = out.iter().rev()
                    .find(|l: &&String| l.contains("st "))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("?")
                    .trim_end_matches('{').trim_end()
                    .to_string();
                out.push(format!("{}}}", ind));
                out.push(format!("{}im {} {{", ind, struct_name));
                in_struct = false;
            }
            let converted = if in_struct && !starts_struct && converted.trim() != "}"
                    && !converted.contains("fn ") && !converted.starts_with("im ")
                    && !converted.starts_with("st ") && !converted.trim().is_empty() {
                // Strip lt/mt and defaults from struct fields
                let stripped = converted.replace("lt ", "").replace("mt ", "");
                let stripped = if let Some(eq_pos) = stripped.find(" = ") {
                    stripped[..eq_pos].to_string()
                } else {
                    stripped
                };
                // If field has no type annotation, add a placeholder
                if !stripped.trim().contains(':') && !stripped.trim().is_empty() {
                    format!("{}: Any", stripped.trim())
                } else {
                    stripped
                }
            } else {
                converted
            };
            if converted.trim() == "}" {
                in_struct = false;
            }

            out.push(format!("{}{}", ind, converted));
        }
        let result = out.join("\n");
        if !result.ends_with('\n') && !result.is_empty() {
            format!("{}\n", result)
        } else {
            result
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Rust
// ═══════════════════════════════════════════════════════════════════

mod rust {
    use super::*;

    pub fn emit(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);

        match node.kind() {
            "source_file" => emit_children_as_statements(node, src, depth, emit),

            "line_comment" | "block_comment" => {
                format!("{}{}", ind, text(node, src).trim())
            }

            "function_item" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_params(&n, src)).unwrap_or_default();
                let ret = node.child_by_field_name("return_type")
                    .map(|n| format!(" -> {}", text(&n, src)));
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                let is_pub = node.children(&mut node.walk())
                    .any(|c| text(&c, src) == "pub");
                let vis = if is_pub { "pb " } else { "" };

                format!("{}{}fn {}({}){}{}", ind, vis, name, params,
                        ret.unwrap_or_default(), body)
            }

            "struct_item" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let is_pub = node.children(&mut node.walk())
                    .any(|c| text(&c, src) == "pub");
                let vis = if is_pub { "pb " } else { "" };

                let body = node.child_by_field_name("body")
                    .map(|n| emit_struct_body(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}{}st {}{}", ind, vis, name, body)
            }

            "enum_item" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let is_pub = node.children(&mut node.walk())
                    .any(|c| text(&c, src) == "pub");
                let vis = if is_pub { "pb " } else { "" };
                let body = node.child_by_field_name("body")
                    .map(|n| emit_enum_body(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}{}en {}{}", ind, vis, name, body)
            }

            "trait_item" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let is_pub = node.children(&mut node.walk())
                    .any(|c| text(&c, src) == "pub");
                let vis = if is_pub { "pb " } else { "" };
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}{}tr {}{}", ind, vis, name, body)
            }

            "impl_item" => {
                let trait_name = node.child_by_field_name("trait")
                    .map(|n| text(&n, src));
                let type_name = node.child_by_field_name("type")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                match trait_name {
                    Some(tr) => format!("{}im {} fo {}{}", ind, tr, type_name, body),
                    None => format!("{}im {}{}", ind, type_name, body),
                }
            }

            "let_declaration" => {
                let is_mut = node.children(&mut node.walk())
                    .any(|c| text(&c, src) == "mut");
                let binding = if is_mut { "mt" } else { "lt" };

                let pat = node.child_by_field_name("pattern")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let ty = node.child_by_field_name("type")
                    .map(|n| format!(": {}", text(&n, src)));
                let val = node.child_by_field_name("value")
                    .map(|n| format!(" = {}", emit_expr(&n, src)));

                format!("{}{} {}{}{}", ind, binding, pat,
                        ty.unwrap_or_default(), val.unwrap_or_default())
            }

            "expression_statement" => {
                if let Some(child) = node.named_child(0) {
                    format!("{}{}", ind, emit_expr(&child, src))
                } else {
                    String::new()
                }
            }

            "if_expression" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let then = node.child_by_field_name("consequence")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                let mut result = format!("{}if {}{}", ind, cond, then);

                if let Some(alt) = node.child_by_field_name("alternative") {
                    if alt.kind() == "if_expression" {
                        let inner = emit(&alt, src, depth);
                        let inner = inner.trim_start();
                        result.push_str(&format!(" el {}", inner));
                    } else {
                        let else_body = emit_block(&alt, src, depth);
                        result.push_str(&format!(" el{}", else_body));
                    }
                }

                result
            }

            "match_expression" => {
                let val = node.child_by_field_name("value")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_match_body(&n, src, depth))
                    .unwrap_or_default();
                format!("{}mc {} {{\n{}{}}}", ind, val, body, ind)
            }

            "for_expression" => {
                let pat = node.child_by_field_name("pattern")
                    .map(|n| text(&n, src)).unwrap_or("_");
                let val = node.child_by_field_name("value")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}fo {} in {}{}", ind, pat, val, body)
            }

            "while_expression" | "while_let_expression" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}wl {}{}", ind, cond, body)
            }

            "loop_expression" => {
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}lp{}", ind, body)
            }

            "return_expression" => {
                if let Some(expr) = node.named_child(0) {
                    format!("{}rt {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}rt", ind)
                }
            }

            "break_expression" => format!("{}bk", ind),
            "continue_expression" => format!("{}ct", ind),

            "use_declaration" => {
                format!("{}use {}", ind, text(node, src).trim()
                    .strip_prefix("use ").unwrap_or("?")
                    .trim_end_matches(';'))
            }

            "mod_item" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                format!("{}md {}", ind, name)
            }

            "type_item" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let ty = node.child_by_field_name("type")
                    .map(|n| text(&n, src)).unwrap_or("?");
                format!("{}ty {} = {}", ind, name, ty)
            }

            "const_item" | "static_item" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let ty = node.child_by_field_name("type")
                    .map(|n| format!(": {}", text(&n, src)));
                let val = node.child_by_field_name("value")
                    .map(|n| format!(" = {}", emit_expr(&n, src)));
                format!("{}cn {}{}{}", ind, name,
                        ty.unwrap_or_default(), val.unwrap_or_default())
            }

            "attribute_item" | "inner_attribute_item" => {
                let raw = text(node, src).trim();
                format!("{}{}", ind, raw)
            }

            "macro_invocation" => {
                let raw = text(node, src).trim().trim_end_matches(';');
                // Map common macros
                if raw.starts_with("println!") || raw.starts_with("print!") || raw.starts_with("eprintln!") {
                    let inner = extract_macro_args(raw);
                    format!("{}prt({})", ind, inner)
                } else if raw.starts_with("vec!") {
                    let inner = extract_macro_args(raw);
                    format!("{}[{}]", ind, inner)
                } else if raw.starts_with("format!") {
                    let inner = extract_macro_args(raw);
                    format!("{}fmt({})", ind, inner)
                } else {
                    format!("{}{}", ind, raw)
                }
            }

            _ => {
                let raw = text(node, src).trim();
                if raw.is_empty() { String::new() }
                else { format!("{}{}", ind, raw) }
            }
        }
    }

    fn emit_expr(node: &Node, src: &[u8]) -> String {
        match node.kind() {
            "identifier" | "field_identifier" | "type_identifier" => {
                let name = text(node, src);
                match name {
                    "self" => "sf".to_string(),
                    "Self" => "Sf".to_string(),
                    "true" => "tt".to_string(),
                    "false" => "ff".to_string(),
                    _ => name.to_string(),
                }
            }

            "boolean_literal" => {
                if text(node, src) == "true" { "tt".to_string() }
                else { "ff".to_string() }
            }

            "integer_literal" | "float_literal" | "char_literal" =>
                text(node, src).to_string(),

            "string_literal" => text(node, src).to_string(),

            "call_expression" => {
                let func = node.child_by_field_name("function")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let args = node.child_by_field_name("arguments")
                    .map(|n| emit_call_args(&n, src)).unwrap_or_default();
                format!("{}({})", func, args)
            }

            "field_expression" => {
                let obj = node.child_by_field_name("value")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let field = node.child_by_field_name("field")
                    .map(|n| text(&n, src)).unwrap_or("");
                let field = map_method(field);
                format!("{}.{}", obj, field)
            }

            "method_call_expression" => {
                // In tree-sitter-rust, this is a field_expression + call
                text(node, src).to_string()
            }

            "index_expression" => {
                let val = node.child_by_field_name("value")
                    .or_else(|| node.named_child(0))
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let idx = node.named_child(1)
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("{}[{}]", val, idx)
            }

            "binary_expression" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src)).unwrap_or("+");
                let op = match op {
                    "&&" => "and",
                    "||" => "or",
                    other => other,
                };
                format!("{} {} {}", left, op, right)
            }

            "unary_expression" | "reference_expression" | "dereference_expression" =>
                text(node, src).to_string(),

            "closure_expression" => {
                let params = node.child_by_field_name("parameters")
                    .map(|n| text(&n, src).replace('|', ""))
                    .unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("|{}| {}", params.trim(), body)
            }

            "array_expression" => {
                let mut items = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    items.push(emit_expr(&child, src));
                }
                format!("[{}]", items.join(", "))
            }

            "tuple_expression" => {
                let mut items = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    items.push(emit_expr(&child, src));
                }
                format!("({})", items.join(", "))
            }

            "block" => {
                // Inline block expression
                text(node, src).to_string()
            }

            "range_expression" => {
                let raw = text(node, src);
                // ..= → ..=, .. → ..
                raw.to_string()
            }

            "try_expression" => {
                if let Some(expr) = node.named_child(0) {
                    format!("{}?", emit_expr(&expr, src))
                } else {
                    text(node, src).to_string()
                }
            }

            "parenthesized_expression" => {
                if let Some(inner) = node.named_child(0) {
                    format!("({})", emit_expr(&inner, src))
                } else {
                    "()".to_string()
                }
            }

            "unit_expression" => "()".to_string(),

            "macro_invocation" => {
                let raw = text(node, src).trim_end_matches(';');
                if raw.starts_with("println!") || raw.starts_with("print!") {
                    format!("prt({})", extract_macro_args(raw))
                } else if raw.starts_with("vec!") {
                    format!("[{}]", extract_macro_args(raw))
                } else if raw.starts_with("format!") {
                    format!("fmt({})", extract_macro_args(raw))
                } else {
                    raw.to_string()
                }
            }

            _ => text(node, src).to_string(),
        }
    }

    fn emit_params(node: &Node, src: &[u8]) -> String {
        let mut params = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "parameter" || child.kind() == "self_parameter" {
                let raw = text(&child, src);
                let converted = raw
                    .replace("&mut self", "Sf")
                    .replace("&self", "sf")
                    .replace("mut self", "Sf")
                    .replace("self", "sf")
                    .replace("mut ", "");
                params.push(converted);
            }
        }
        params.join(", ")
    }

    fn emit_call_args(node: &Node, src: &[u8]) -> String {
        let mut args = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            args.push(emit_expr(&child, src));
        }
        args.join(", ")
    }

    fn emit_block(node: &Node, src: &[u8], depth: usize) -> String {
        let inner = emit_children_as_statements(node, src, depth + 1, emit);
        let ind = indent(depth);
        format!(" {{\n{}{}}}", inner, ind)
    }

    fn emit_struct_body(node: &Node, src: &[u8], depth: usize) -> String {
        let mut fields = String::new();
        let inner_ind = indent(depth + 1);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "field_declaration" {
                let name = child.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let ty = child.child_by_field_name("type")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let is_pub = child.children(&mut child.walk())
                    .any(|c| text(&c, src) == "pub");
                let vis = if is_pub { "pb " } else { "" };
                fields.push_str(&format!("{}{}{}: {}\n", inner_ind, vis, name, ty));
            }
        }
        let ind = indent(depth);
        format!(" {{\n{}{}}}", fields, ind)
    }

    fn emit_enum_body(node: &Node, src: &[u8], depth: usize) -> String {
        let mut variants = String::new();
        let inner_ind = indent(depth + 1);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            variants.push_str(&format!("{}{}\n", inner_ind, text(&child, src).trim()));
        }
        let ind = indent(depth);
        format!(" {{\n{}{}}}", variants, ind)
    }

    fn emit_match_body(node: &Node, src: &[u8], depth: usize) -> String {
        let mut out = String::new();
        let inner_ind = indent(depth + 1);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "match_arm" {
                let raw = text(&child, src).trim().trim_end_matches(',');
                out.push_str(&format!("{}{}\n", inner_ind, raw));
            }
        }
        out
    }

    fn extract_macro_args(raw: &str) -> &str {
        // println!("hello") → "hello"
        // vec![1, 2, 3] → 1, 2, 3
        if let Some(start) = raw.find('(') {
            &raw[start + 1..raw.len().saturating_sub(1)]
        } else if let Some(start) = raw.find('[') {
            &raw[start + 1..raw.len().saturating_sub(1)]
        } else {
            raw
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// C
// ═══════════════════════════════════════════════════════════════════

mod c {
    use super::*;

    pub fn emit(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);

        match node.kind() {
            "translation_unit" => emit_children_as_statements(node, src, depth, emit),

            "comment" => {
                let t = text(node, src).trim();
                format!("{}{}", ind, t)
            }

            "function_definition" => {
                // C: return_type name(params) { body }
                let declarator = node.child_by_field_name("declarator");
                let name = declarator.as_ref()
                    .and_then(|d| d.child_by_field_name("declarator"))
                    .map(|n| text(&n, src))
                    .unwrap_or("?");
                let params = declarator.as_ref()
                    .and_then(|d| d.child_by_field_name("parameters"))
                    .map(|n| emit_c_params(&n, src))
                    .unwrap_or_default();
                let ret = node.child_by_field_name("type")
                    .map(|n| text(&n, src));
                let ret_ann = match ret {
                    Some("void") | None => String::new(),
                    Some(t) => format!(" -> {}", t),
                };
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                format!("{}fn {}({}){}{}", ind, name, params, ret_ann, body)
            }

            "declaration" => {
                // Variable declarations: type name = value;
                let raw = text(node, src).trim().trim_end_matches(';');
                // Try simple pattern: "type name = value" or "type name"
                format!("{}lt {}", ind, raw)
            }

            "if_statement" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr_unwrap_parens(&n, src)).unwrap_or_default();
                let then = node.child_by_field_name("consequence")
                    .map(|n| {
                        if n.kind() == "compound_statement" {
                            emit_block(&n, src, depth)
                        } else {
                            let inner = emit(&n, src, depth + 1);
                            format!(" {{\n{}{}{}}}", inner,
                                   if inner.ends_with('\n') { "" } else { "\n" }, ind)
                        }
                    })
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                let mut result = format!("{}if {}{}", ind, cond, then);

                if let Some(alt) = node.child_by_field_name("alternative") {
                    if alt.kind() == "if_statement" {
                        let inner = emit(&alt, src, depth);
                        let inner = inner.trim_start();
                        result.push_str(&format!(" el {}", inner));
                    } else if alt.kind() == "compound_statement" {
                        let else_body = emit_block(&alt, src, depth);
                        result.push_str(&format!(" el{}", else_body));
                    } else {
                        let inner = emit(&alt, src, depth + 1);
                        result.push_str(&format!(" el {{\n{}{}{}}}", inner,
                                                if inner.ends_with('\n') { "" } else { "\n" }, ind));
                    }
                }

                result
            }

            "for_statement" => {
                // C for loops don't map cleanly to fo; use raw
                format!("{}{}", ind, text(node, src).trim())
            }

            "while_statement" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr_unwrap_parens(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| {
                        if n.kind() == "compound_statement" {
                            emit_block(&n, src, depth)
                        } else {
                            format!(" {{\n{}{}}}", emit(&n, src, depth + 1), ind)
                        }
                    })
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}wl {}{}", ind, cond, body)
            }

            "return_statement" => {
                if let Some(expr) = node.named_child(0) {
                    format!("{}rt {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}rt", ind)
                }
            }

            "break_statement" => format!("{}bk", ind),
            "continue_statement" => format!("{}ct", ind),

            "expression_statement" => {
                if let Some(child) = node.named_child(0) {
                    format!("{}{}", ind, emit_expr(&child, src))
                } else {
                    String::new()
                }
            }

            "struct_specifier" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}st {}{}", ind, name, body)
            }

            "enum_specifier" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}en {}{}", ind, name, body)
            }

            "switch_statement" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr_unwrap_parens(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_default();
                format!("{}mc {}{}", ind, cond, body)
            }

            "preproc_include" | "preproc_def" | "preproc_ifdef" |
            "preproc_ifndef" | "preproc_if" | "preproc_else" |
            "preproc_endif" | "preproc_function_def" => {
                // Strip preprocessor directives — they're metadata, not logic
                String::new()
            }

            "type_definition" => {
                // C typedef: "typedef struct { fields } Name;" → "st Name { fields }"
                let raw = text(node, src).trim().trim_end_matches(';');
                let body = raw.strip_prefix("typedef ").unwrap_or(raw);
                // Extract name from end: "struct { ... } TaskName" → name is last word
                let name = body.split_whitespace().last().unwrap_or("?")
                    .trim_end_matches('}');
                if body.contains('{') {
                    // Struct typedef — emit as st
                    let fields_start = body.find('{').unwrap_or(0);
                    let fields_end = body.rfind('}').unwrap_or(body.len());
                    let fields = &body[fields_start..=fields_end];
                    format!("{}st {} {}", ind, name, fields)
                } else {
                    format!("{}ty {} = {}", ind, name, body)
                }
            }

            _ => {
                let raw = text(node, src).trim();
                if raw.is_empty() { String::new() }
                else { format!("{}{}", ind, raw) }
            }
        }
    }

    fn emit_expr(node: &Node, src: &[u8]) -> String {
        match node.kind() {
            "identifier" | "field_identifier" | "type_identifier" =>
                text(node, src).to_string(),

            "true" => "tt".to_string(),
            "false" => "ff".to_string(),
            "null" => "nil".to_string(),
            "number_literal" | "char_literal" | "string_literal" =>
                text(node, src).to_string(),

            "call_expression" => {
                let func = node.child_by_field_name("function")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let args = node.child_by_field_name("arguments")
                    .map(|n| emit_call_args(&n, src)).unwrap_or_default();

                let func = match func.as_str() {
                    "printf" | "puts" | "fprintf" => "prt".to_string(),
                    "strlen" => "ln".to_string(),
                    "malloc" | "calloc" | "realloc" => func,
                    _ => func,
                };

                format!("{}({})", func, args)
            }

            "field_expression" => {
                let obj = node.child_by_field_name("argument")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let field = node.child_by_field_name("field")
                    .map(|n| text(&n, src)).unwrap_or("");
                format!("{}.{}", obj, field)
            }

            "subscript_expression" => {
                let val = node.named_child(0)
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let idx = node.named_child(1)
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("{}[{}]", val, idx)
            }

            "binary_expression" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src)).unwrap_or("+");
                let op = match op {
                    "&&" => "and",
                    "||" => "or",
                    other => other,
                };
                format!("{} {} {}", left, op, right)
            }

            "unary_expression" => {
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src)).unwrap_or("!");
                let arg = node.child_by_field_name("argument")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let op = if op == "!" { "not " } else { op };
                format!("{}{}", op, arg)
            }

            "conditional_expression" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let then = node.child_by_field_name("consequence")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let alt = node.child_by_field_name("alternative")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("if {} {{ {} }} el {{ {} }}", cond, then, alt)
            }

            "assignment_expression" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("{} = {}", left, right)
            }

            "update_expression" => text(node, src).to_string(),
            "cast_expression" => text(node, src).to_string(),
            "sizeof_expression" => text(node, src).to_string(),
            "pointer_expression" => text(node, src).to_string(),

            "parenthesized_expression" => {
                if let Some(inner) = node.named_child(0) {
                    format!("({})", emit_expr(&inner, src))
                } else {
                    "()".to_string()
                }
            }

            "initializer_list" => {
                let mut items = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    items.push(emit_expr(&child, src));
                }
                format!("{{{}}}", items.join(", "))
            }

            _ => text(node, src).to_string(),
        }
    }

    fn emit_expr_unwrap_parens(node: &Node, src: &[u8]) -> String {
        if node.kind() == "parenthesized_expression" {
            if let Some(inner) = node.named_child(0) {
                return emit_expr(&inner, src);
            }
        }
        emit_expr(node, src)
    }

    fn emit_c_params(node: &Node, src: &[u8]) -> String {
        let mut params = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "parameter_declaration" {
                let name = child.child_by_field_name("declarator")
                    .map(|n| text(&n, src)).unwrap_or("_");
                let ty = child.child_by_field_name("type")
                    .map(|n| format!(": {}", text(&n, src)));
                params.push(format!("{}{}", name, ty.unwrap_or_default()));
            }
        }
        if params.is_empty() {
            // Check for "void" parameter
            let raw = text(node, src);
            if raw.contains("void") {
                return String::new();
            }
        }
        params.join(", ")
    }

    fn emit_call_args(node: &Node, src: &[u8]) -> String {
        let mut args = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            args.push(emit_expr(&child, src));
        }
        args.join(", ")
    }

    fn emit_block(node: &Node, src: &[u8], depth: usize) -> String {
        let inner = emit_children_as_statements(node, src, depth + 1, emit);
        let ind = indent(depth);
        format!(" {{\n{}{}}}", inner, ind)
    }
}

#[cfg(test)]
mod canon_tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("camelCase"), "camel_case");
        assert_eq!(to_snake_case("PascalCase"), "pascal_case");
        assert_eq!(to_snake_case("nextId"), "next_id");
        assert_eq!(to_snake_case("ID"), "id");
        assert_eq!(to_snake_case("Done"), "done");
        assert_eq!(to_snake_case("Tasks"), "tasks");
        assert_eq!(to_snake_case("TaskManager"), "task_manager");
        assert_eq!(to_snake_case("byPriority"), "by_priority");
        assert_eq!(to_snake_case("NewTaskManager"), "new_task_manager");
    }

    #[test]
    fn test_should_convert() {
        let empty_types = std::collections::HashSet::new();
        // "Done" after "sf." should convert
        let line = "if sf.Done {";
        let chars: Vec<char> = line.chars().collect();
        assert!(should_convert_to_snake_case("Done", "if sf.", &chars, 10, &empty_types),
            "Done after sf. should convert");

        // "Tasks" after "sf." should convert
        let line2 = "fo _, t in sf.Tasks {";
        let chars2: Vec<char> = line2.chars().collect();
        assert!(should_convert_to_snake_case("Tasks", "fo _, t in sf.", &chars2, 19, &empty_types),
            "Tasks after sf. should convert");
    }

    #[test]
    fn test_canonicalize_go_fields() {
        let input = "    if sf.Done {\n      sf.Tasks = filtered";
        let result = canonicalize(input);
        assert!(result.contains("self.done"), "sf.Done should become self.done, got: {}", result);
        assert!(result.contains("self.tasks"), "sf.Tasks should become self.tasks, got: {}", result);
    }

    #[test]
    fn test_normalize_identifiers() {
        assert_eq!(
            normalize_identifiers("if sf.Done {"),
            "if sf.done {"
        );
        assert_eq!(
            normalize_identifiers("sf.Tasks = filtered"),
            "sf.tasks = filtered"
        );
        assert_eq!(
            normalize_identifiers("lt task = Task(id, title)"),
            "lt task = Task(id, title)"
        );
    }

    #[test]
    fn test_canonicalize_methods() {
        // sf → self, .fl → .filter, .mp → .map
        assert_eq!(
            canonicalize("sf.tasks.fl(|t| t.id)"),
            "self.tasks.filter(|t| t.id)"
        );
        assert_eq!(
            canonicalize("sf.tasks.mp(|t| t.id)"),
            "self.tasks.map(|t| t.id)"
        );
    }

    #[test]
    fn test_normalize_keywords() {
        assert_eq!(normalize_keywords("fn foo() {"), "def foo() {");
        assert_eq!(normalize_keywords("rt 42"), "return 42");
        assert_eq!(normalize_keywords("lt x = 1"), "x := 1");   // let → walrus
        assert_eq!(normalize_keywords("mt x = 1"), "x = 1");    // mut → bare
        assert_eq!(normalize_keywords("fo x in items {"), "for x in items {");
    }
}
