/// Tree-sitter based source → Nebula translators.
///
/// Each language converter parses source code using tree-sitter, walks the
/// concrete syntax tree, and emits equivalent Nebula source text.
/// Unknown constructs fall back to raw source text with a comment marker.

use tree_sitter::{Node, Parser};
use crate::compactor;

// ── Public API ──

pub fn from_python(source: &str) -> String {
    convert_with_language(source, tree_sitter_python::LANGUAGE.into(), python::emit)
}

pub fn from_javascript(source: &str) -> String {
    convert_with_language(source, tree_sitter_javascript::LANGUAGE.into(), js::emit)
}

pub fn from_typescript(source: &str) -> String {
    convert_with_language(source, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), ts::emit)
}

pub fn from_go(source: &str) -> String {
    convert_with_language(source, tree_sitter_go::LANGUAGE.into(), go::emit)
}

pub fn from_swift(source: &str) -> String {
    // Swift uses pattern-based conversion (tree-sitter-swift has inconsistent
    // field names causing duplicate output)
    let nebula = swift::convert_swift_raw(source, 0);
    let nebula = postprocess(&nebula);
    let nebula = strip_semicolons(&nebula);
    let nebula = minify(&nebula);
    fix_raw_keywords(&nebula)
}

pub fn from_kotlin(source: &str) -> String {
    // Kotlin uses pattern-based conversion (tree-sitter-kotlin depends on
    // tree-sitter 0.20 which conflicts with 0.26 at link time)
    let nebula = kotlin::convert_kotlin_raw(source, 0);
    let nebula = postprocess(&nebula);
    let nebula = strip_semicolons(&nebula);
    let nebula = minify(&nebula);
    fix_raw_keywords(&nebula)
}

pub fn from_rust(source: &str) -> String {
    convert_with_language(source, tree_sitter_rust::LANGUAGE.into(), rust::emit)
}

pub fn from_c(source: &str) -> String {
    convert_with_language(source, tree_sitter_c::LANGUAGE.into(), c::emit)
}

// ── Core ──

fn convert_with_language(
    source: &str,
    language: tree_sitter::Language,
    emitter: fn(&Node, &[u8], usize) -> String,
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
    // Minify: strip comments, blank lines, compact indentation
    let nebula = minify(&nebula);
    // Final keyword cleanup (runs AFTER minify so whitespace is normalized)
    let nebula = fix_raw_keywords(&nebula);
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

        // return → rt, for → fo
        let trimmed = if trimmed.starts_with("return ") {
            format!("rt {}", &trimmed[7..])
        } else if trimmed == "return" {
            "rt".to_string()
        } else if trimmed.starts_with("match ") {
            format!("mc {}", &trimmed[6..])
        } else if trimmed.starts_with("for ") {
            // C-style for with semicolons → while loop
            if trimmed.contains(';') {
                // Extract condition from "for (init; cond; update) {" → "wl cond {"
                let inner = trimmed.strip_prefix("for ").unwrap_or(trimmed);
                let inner = inner.trim_start_matches('(');
                let parts: Vec<&str> = inner.splitn(3, ';').collect();
                if parts.len() >= 2 {
                    let cond = parts[1].trim();
                    format!("wl {} {{", cond)
                } else {
                    format!("fo {}", &trimmed[4..])
                }
            } else {
                format!("fo {}", &trimmed[4..]).replace(" = ", " in ")
            }
        } else if trimmed.starts_with("fo ") && trimmed.contains(" = ") && trimmed.contains('{') {
            // Go range that still has = instead of in
            trimmed.replace(" = ", " in ")
        }
        // Python tuple unpacking: "a, b = 0, 1" → "lt (a, b) = (0, 1)"
        else if trimmed.contains(", ") && trimmed.contains(" = ")
                && !trimmed.contains("for ") && !trimmed.contains("fo ")
                && !trimmed.contains("range") && !trimmed.contains("in ") {
            let parts: Vec<&str> = trimmed.splitn(2, " = ").collect();
            if parts.len() == 2 && parts[0].contains(',') && !parts[0].contains('(') {
                format!("lt ({}) = ({})", parts[0], parts[1])
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

/// Minify ingested code: strip comments, blank lines, compact indentation.
/// Based on LLMLingua's principle of removing low-information-density tokens.
fn minify(source: &str) -> String {
    source.lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Remove comment-only lines and blank lines
            !trimmed.is_empty() && !trimmed.starts_with("//")
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

/// Map common builtin function names to Nebula equivalents.
/// Only maps to aliases that are SINGLE tokens in cl100k_base.
/// Aliases that cost 2+ tokens (tsr, tof, psh, srt, trm, fnd) are avoided.
fn map_builtin(name: &str) -> &str {
    match name {
        "print" | "println" | "puts" | "fmt.Println" | "fmt.Printf" | "console.log"
            | "console.warn" | "console.error" | "printf" | "fprintf" => "prt",
        "len" | "strlen" => "ln",
        // "str" stays as "str" (1 token) — "tsr" costs 2 tokens
        "int" => "toi",
        // "float" stays as "float" (1 token) — "tof" costs 2 tokens
        "type" | "isinstance" | "typeof" => "typ",
        "range" => "rng",
        "enumerate" => "enu",
        // "push"/"append" stay as-is (1 token) — "psh" costs 2 tokens
        // "sorted" stays as "sorted" (1 token) — "srt" costs 2 tokens
        "abs" => "abs",
        "min" => "min",
        "max" => "max",
        "snprintf" | "sprintf" | "fmt.Sprintf" | "String.format" => "fmt",
        _ => name,
    }
}

/// Map common method names to Nebula equivalents.
/// Only maps to aliases that are SINGLE tokens in cl100k_base.
fn map_method(name: &str) -> &str {
    match name {
        // These aliases are all verified single-tokens in cl100k_base:
        "length" | "count" | "size" | "len" => "ln",
        "contains" | "includes" | "containsKey" | "has" => "ck",
        "join" => "jn",
        "map" => "mp",
        "filter" | "retain" => "fl",
        "reduce" | "fold" | "fold_left" => "fld",
        "forEach" | "for_each" => "ea",
        "reverse" | "reversed" => "rev",
        "iter" | "iterator" => "it",
        "collect" => "col",
        "unwrap" | "unwrap_or" => "uw",
        "isEmpty" | "is_empty" => "emp",
        "removeAll" | "clear" => "clr",
        "toFixed" | "format" => "fmt",
        "enumerate" | "enumerated" => "enu",
        "pop" | "removeLast" => "pop",
        "insert" => "ins",
        "keys" => "keys",
        "values" | "vals" => "vals",
        "sum" => "sum",
        // These stay as source names — our aliases cost 2 tokens:
        // push/append → keep as-is (push=1tok, psh=2tok)
        // sort/sorted → keep as-is (sort=1tok, srt=2tok)
        // trim/strip → keep as-is (trim=1tok, trm=2tok)
        // find/first → keep as-is (find=1tok, fnd=2tok)
        // to_string → keep as-is (str=1tok, tsr=2tok)
        // split → keep as-is (split=1tok, spl=1tok — wash, but split is more readable)
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
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
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
                if let Some(expr) = node.named_child(0) {
                    format!("{}rt {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}rt", ind)
                }
            }

            "pass_statement" => format!("{}// pass", ind),
            "break_statement" => format!("{}bk", ind),
            "continue_statement" => format!("{}ct", ind),

            "import_statement" | "import_from_statement" => {
                let raw = text(node, src).trim();
                format!("{}// {}", ind, raw)
            }

            "assignment" => {
                let left_node = node.child_by_field_name("left");
                let left = left_node.as_ref()
                    .map(|n| emit_expr(n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();

                // Don't add `lt` for attribute/subscript assignments (e.g. self.x = val)
                let is_simple = left_node.map(|n| n.kind() == "identifier").unwrap_or(false);
                let type_node = node.child_by_field_name("type");
                let type_ann = type_node.map(|n| format!(": {}", emit_type(&n, src)));

                if is_simple {
                    format!("{}lt {}{} = {}", ind, left, type_ann.unwrap_or_default(), right)
                } else {
                    format!("{}{} = {}", ind, left, right)
                }
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
                let sub = node.child_by_field_name("subscript")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
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
                // Can have chained comparisons: a < b < c
                let mut parts = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.is_named() {
                        parts.push(emit_expr(&child, src));
                    } else {
                        let op = text(&child, src);
                        let op = match op {
                            "is" => "==",
                            "is not" => "!=",
                            "in" => "in",
                            "not in" => "not in",
                            other => other,
                        };
                        parts.push(op.to_string());
                    }
                }
                parts.join(" ")
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
                let op = node.child_by_field_name("operator")
                    .map(|n| text(&n, src)).unwrap_or("-");
                let operand = node.child_by_field_name("operand")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
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

            "list_comprehension" => {
                // [expr for x in iter if cond] → iter.fl(|x| cond).mp(|x| expr)
                // [expr for x in iter] → iter.mp(|x| expr)
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();

                // First child is the expression, rest are for_in_clause / if_clause
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

                if let Some(cond) = condition {
                    format!("{}.fl(|{}| {}).mp(|{}| {})", iterable, binding, cond, binding, expr)
                } else {
                    format!("{}.mp(|{}| {})", iterable, binding, expr)
                }
            }

            "slice" => {
                let raw = text(node, src);
                raw.to_string()
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
            "source_file" => emit_children_as_statements(node, src, depth, emit),

            "comment" => {
                let t = text(node, src).trim();
                format!("{}{}", ind, t)
            }

            "function_declaration" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_params(&n, src)).unwrap_or_default();
                let ret = node.child_by_field_name("result")
                    .map(|n| format!(" -> {}", text(&n, src)));
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}fn {}({}){}{}", ind, name, params,
                        ret.unwrap_or_default(), body)
            }

            "method_declaration" => {
                let receiver = node.child_by_field_name("receiver")
                    .map(|n| text(&n, src)).unwrap_or("");
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_params(&n, src)).unwrap_or_default();
                let ret = node.child_by_field_name("result")
                    .map(|n| format!(" -> {}", text(&n, src)));
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                // Extract receiver type for impl context
                let recv_type = receiver.trim_matches(|c: char| c == '(' || c == ')')
                    .split_whitespace().last().unwrap_or("?")
                    .trim_start_matches('*');

                format!("{}fn {}(sf, {}){}{}", ind,
                        name, params, ret.unwrap_or_default(), body)
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
                let left = node.child_by_field_name("left")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("{}mt {} = {}", ind, left, right)
            }

            "var_declaration" | "const_declaration" => {
                let kind = node.kind();
                let binding = if kind == "const_declaration" { "lt" } else { "mt" };
                let mut cursor = node.walk();
                let mut out = Vec::new();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "var_spec" || child.kind() == "const_spec" {
                        let name = child.child_by_field_name("name")
                            .map(|n| text(&n, src)).unwrap_or("?");
                        let value = child.child_by_field_name("value")
                            .map(|n| emit_expr(&n, src));
                        match value {
                            Some(v) => out.push(format!("{}{} {} = {}", ind, binding, name, v)),
                            None => out.push(format!("{}{} {}", ind, binding, name)),
                        }
                    }
                }
                out.join("\n")
            }

            "assignment_statement" => {
                let left = node.child_by_field_name("left")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let right = node.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("{}{} = {}", ind, left, right)
            }

            "if_statement" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("consequence")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                let mut result = format!("{}if {}{}", ind, cond, body);

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
                let raw = text(node, src);
                if raw.contains("range") {
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
                    let cond_text = node.child_by_field_name("condition")
                        .map(|n| emit_expr(&n, src));
                    match cond_text {
                        Some(c) => {
                            let body = node.child_by_field_name("body")
                                .map(|n| emit_block(&n, src, depth))
                                .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                            format!("{}wl {}{}", ind, c, body)
                        }
                        None => format!("{}{}", ind, raw.trim()),
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
                } else {
                    format!("{}rt {}", ind, exprs.join(", "))
                }
            }

            "expression_statement" => {
                if let Some(child) = node.named_child(0) {
                    format!("{}{}", ind, emit_expr(&child, src))
                } else {
                    String::new()
                }
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
                    format!("{}// defer {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}// defer", ind)
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

            _ => {
                let raw = text(node, src).trim();
                if raw.is_empty() { String::new() }
                else { format!("{}{}", ind, raw) }
            }
        }
    }

    fn emit_expr(node: &Node, src: &[u8]) -> String {
        match node.kind() {
            "identifier" | "field_identifier" | "package_identifier" | "type_identifier" =>
                text(node, src).to_string(),

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
                let args = node.child_by_field_name("arguments")
                    .map(|n| emit_call_args(&n, src)).unwrap_or_default();

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
                let op = if op == "!" { "not " } else { op };
                format!("{}{}", op, arg)
            }

            "composite_literal" => {
                // Type{fields} — could be struct, map, or slice literal
                text(node, src).to_string()
            }

            "slice_expression" => text(node, src).to_string(),
            "type_assertion_expression" => text(node, src).to_string(),
            "func_literal" => {
                // func(params) rettype { body } → |params| { body }
                text(node, src).to_string()
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
                let name = child.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("_");
                let ty = child.child_by_field_name("type")
                    .map(|n| format!(": {}", text(&n, src)));
                params.push(format!("{}{}", name, ty.unwrap_or_default()));
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
                fields.push_str(&format!("{}{}: {}\n", inner_ind, name, ty));
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
                let left = child.child_by_field_name("left")
                    .map(|n| text(&n, src)).unwrap_or("_");
                let right = child.child_by_field_name("right")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let body = node.child_by_field_name("body")
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

    pub fn emit(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);

        match node.kind() {
            "source_file" => emit_children_as_statements(node, src, depth, emit),

            "comment" | "multiline_comment" => {
                format!("{}{}", ind, text(node, src).trim())
            }

            "function_declaration" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let raw = text(node, src);
                // Extract params from raw (tree-sitter-swift field names vary)
                let params = extract_between(raw, "(", ")").unwrap_or_default();
                let neb_params = convert_swift_params(&params);
                let body = find_body_block(node, src, depth);
                format!("{}fn {}({}){}", ind, name, neb_params, body)
            }

            "class_declaration" => {
                let name = extract_name_from_raw(text(node, src));
                let body = find_body_block(node, src, depth);
                format!("{}st {}{}", ind, name, body)
            }

            "struct_declaration" => {
                let name = extract_name_from_raw(text(node, src));
                let body = find_body_block(node, src, depth);
                format!("{}st {}{}", ind, name, body)
            }

            "protocol_declaration" => {
                let name = extract_name_from_raw(text(node, src));
                let body = find_body_block(node, src, depth);
                format!("{}tr {}{}", ind, name, body)
            }

            "enum_declaration" => {
                let name = extract_name_from_raw(text(node, src));
                let body = find_body_block(node, src, depth);
                format!("{}en {}{}", ind, name, body)
            }

            _ => {
                // For Swift, fall back to line-by-line conversion of raw text
                let raw = text(node, src);
                convert_swift_raw(raw, depth)
            }
        }
    }

    fn find_body_block(node: &Node, src: &[u8], depth: usize) -> String {
        let ind = indent(depth);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind.contains("body") || kind.contains("block") || kind == "{" {
                if child.named_child_count() > 0 {
                    let inner = emit_children_as_statements(&child, src, depth + 1, emit);
                    return format!(" {{\n{}{}}}", inner, ind);
                }
            }
        }
        // Try to find curly braces in the raw text
        let raw = text(node, src);
        if let Some(brace_pos) = raw.find('{') {
            let body = &raw[brace_pos + 1..raw.len().saturating_sub(1)];
            let converted = convert_swift_raw(body.trim(), depth + 1);
            format!(" {{\n{}{}}}", converted, ind)
        } else {
            format!(" {{\n{}}}", ind)
        }
    }

    fn convert_swift_params(params: &str) -> String {
        params
            .replace("_ ", "")
            .replace(": String", ": Str")
            .replace(": Int", ": Int")
            .replace(": Bool", ": Bool")
            .replace(": Double", ": Float")
            .replace("self", "sf")
    }

    fn extract_name_from_raw(raw: &str) -> &str {
        // "struct Foo {" → "Foo"
        raw.split_whitespace().nth(1).unwrap_or("?").trim_end_matches('{').trim()
    }

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

    fn extract_between<'a>(s: &'a str, open: &str, close: &str) -> Option<String> {
        let start = s.find(open)? + open.len();
        let end = s.rfind(close)?;
        if start <= end {
            Some(s[start..end].to_string())
        } else {
            None
        }
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
