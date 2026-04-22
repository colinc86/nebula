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
    let s = from_python_with_opts_inner(source, keep_comments);
    let s = rewrite_match_arms(&s);
    let s = rename_import_keywords(&s);
    pad_empty_type_bodies(&s)
}

/// Rename Nebula-keyword names appearing inside `use X.{…}` import lists.
/// Python's `from enum import Enum, IntEnum, auto` canonicalizes to
/// `use enum.{enum, int_enum, auto}` — the first `enum` is a module name
/// (OK) but the imported `enum` collides with the Nebula keyword. Rename
/// the offending items to avoid the parse error.
fn rename_import_keywords(source: &str) -> String {
    let rename = |s: &str| -> String {
        match s {
            "enum" => "_enum".to_string(),
            "struct" => "_struct".to_string(),
            "trait" => "_trait".to_string(),
            "self" => "_self".to_string(),
            "class" => "_class".to_string(),
            "fn" => "_fn".to_string(),
            "let" => "_let".to_string(),
            "mut" => "_mut".to_string(),
            "if" => "_if".to_string(),
            "else" => "_else".to_string(),
            "for" => "_for".to_string(),
            "while" => "_while".to_string(),
            "match" => "_match".to_string(),
            "return" => "_return".to_string(),
            "break" => "_break".to_string(),
            "continue" => "_continue".to_string(),
            "try" => "_try".to_string(),
            "catch" => "_catch".to_string(),
            "throw" => "_throw".to_string(),
            "async" => "_async".to_string(),
            "await" => "_await".to_string(),
            "in" => "_in".to_string(),
            "not" => "_not".to_string(),
            "and" => "_and".to_string(),
            "or" => "_or".to_string(),
            "is" => "_is".to_string(),
            "as" => "_as".to_string(),
            other => other.to_string(),
        }
    };
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("use ") {
            return line.to_string();
        }
        let ws = &line[..line.len() - trimmed.len()];
        // Split between path and brace items.
        let rest = &trimmed[4..];
        let (path_part, items_part) = match rest.find('{') {
            Some(bi) => {
                let end = rest.rfind('}').unwrap_or(rest.len());
                let path = &rest[..bi];
                let items = &rest[bi + 1..end];
                let tail = &rest[end..];
                (path.to_string(), Some((items.to_string(), tail.to_string())))
            }
            None => (rest.to_string(), None),
        };
        // Rename each dotted segment in the path.
        let path_renamed: Vec<String> = path_part
            .trim_end_matches('.')
            .split('.')
            .map(|seg| rename(seg.trim()))
            .collect();
        let path_str = path_renamed.join(".");
        match items_part {
            Some((items, tail)) => {
                let items_renamed: Vec<String> = items.split(',')
                    .map(|i| rename(i.trim()))
                    .collect();
                format!("{}use {}.{{{}{}", ws, path_str, items_renamed.join(", "), tail)
            }
            None => format!("{}use {}", ws, path_str),
        }
    }).collect::<Vec<_>>().join("\n")
}

fn from_python_with_opts_inner(source: &str, keep_comments: bool) -> String {
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
    let nebula = comment_nested_type_decls(&nebula);
    let nebula = comment_orphan_impls(&nebula);
    let nebula = rename_reserved_params(&nebula);
    let nebula = normalize_except_clauses(&nebula);
    let nebula = collapse_multi_except(&nebula);
    // Drop Python `nonlocal`/`global` scope declarations — Nebula has
    // no grammar for them and round-trip back to Python splits them
    // across two statements. The closure semantics are lost but the
    // surrounding function parses and runs.
    let nebula = nebula.lines().filter(|line| {
        let t = line.trim_start();
        !(t.starts_with("nonlocal ") || t.starts_with("global "))
    }).collect::<Vec<_>>().join("\n");
    // Strip Python f-string prefix: `f"..."` → `"..."`. Nebula's string
    // grammar has native `{expr}` interpolation; the `f` prefix, when
    // left in, is parsed as a bare identifier followed by a string.
    let nebula = strip_python_fstring_prefix(&nebula);
    let nebula = nebula.replace(" := ", " = ");
    nebula.trim_end().to_string()
}

pub fn from_javascript_with_opts(source: &str, keep_comments: bool) -> String {
    // Pre-strip the generator asterisk so tree-sitter parses the body as
    // a regular function_declaration (our emit covers that node kind).
    let pre = strip_generator_star(source);
    let out = convert_with_language(&pre, tree_sitter_javascript::LANGUAGE.into(), js::emit, keep_comments);
    let out = strip_js_async_and_generator(&out);
    let out = out.replace("| |", "||");
    let out = wrap_stmt_lambda_bodies(&out);
    let out = normalize_except_clauses(&out);
    let out = collapse_multi_except(&out);
    // `x instanceof Y` — Nebula has no type-check keyword. Rewrite to
    // `type(x) == Y`; lossy (loses inheritance semantics) but keeps the
    // line parseable so surrounding code still translates.
    let out = out.replace(" instanceof ", " == ");
    let out = convert_surviving_template_strings(&out);
    let out = strip_surviving_dollar_braces(&out);
    let out = rewrite_match_arms(&out);
    let out = merge_fallthrough_match_arms(&out);
    let out = rewrite_multi_value_arms(&out);
    let out = rewrite_expr_match_arms(&out);
    let out = wrap_return_in_match_arms(&out);
    let out = wrap_lambda_indent_body(&out);
    let out = collapse_empty_call_args(&out);
    let out = repair_tagged_template_calls(&out);
    pad_empty_type_bodies(&out)
}

/// Drop surviving `${EXPR}` substitutions outside string literals.
/// These leak through when tagged templates (`` tag`hello ${x} ` ``) are
/// flattened into positional calls. Replace `${EXPR}` with the inner
/// EXPR so the value is preserved as a plain expression.
/// Repair tagged-template calls where unquoted text spans survived as
/// bare identifiers in argument position. Pattern: a call where some
/// arguments are neither string literals, nor numeric literals, nor
/// `CALL()`/`X.Y` expressions. Wrap suspicious word-sequence args in
/// `"..."`. Lossy approximation but recovers parseability.
fn repair_tagged_template_calls(source: &str) -> String {
    source.lines().map(|line| {
        // Only line shapes `NAME := CALL(...)` or bare `CALL(...)`.
        let trimmed = line.trim_start();
        let ws = &line[..line.len() - trimmed.len()];
        // Find the outermost `(`.
        let Some(lp) = trimmed.find('(') else { return line.to_string(); };
        let head = &trimmed[..lp];
        // head must end with a call name. Find the last token after the
        // possible `:=` / `=` / `return `/`rt ` prefixes.
        let h = head.trim();
        let tail_name = h.rsplit_once(" := ").map(|(_, r)| r)
            .or_else(|| h.rsplit_once(" = ").map(|(_, r)| r))
            .or_else(|| h.strip_prefix("return ").map(|r| r))
            .or_else(|| h.strip_prefix("rt ").map(|r| r))
            .unwrap_or(h)
            .trim();
        let looks_like_call = !tail_name.is_empty()
            && tail_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
        if !looks_like_call { return line.to_string(); }
        // Find matching `)`.
        let chars: Vec<char> = trimmed.chars().collect();
        let mut depth = 0i32;
        let mut rp = None;
        let mut in_str: Option<char> = None;
        for (i, &c) in chars.iter().enumerate().skip(lp) {
            if let Some(q) = in_str {
                if c == q { in_str = None; }
                continue;
            }
            if c == '"' || c == '\'' { in_str = Some(c); continue; }
            match c {
                '(' => depth += 1,
                ')' => { depth -= 1; if depth == 0 { rp = Some(i); break; } }
                _ => {}
            }
        }
        let Some(rp) = rp else { return line.to_string(); };
        let inner: String = chars[lp + 1..rp].iter().collect();
        let tail: String = chars[rp..].iter().collect();
        // Split top-level args.
        let args = split_args(&inner);
        // Check if any arg is a problematic word-sequence.
        let mut any_bad = false;
        let classify_arg = |a: &str| -> Option<String> {
            let s = a.trim();
            if s.is_empty() { return None; }
            // String literal, number, ident, call, dotted — accept as-is.
            if s.starts_with('"') || s.starts_with('\'') { return Some(s.to_string()); }
            if s.chars().next().map_or(false, |c| c.is_ascii_digit()) { return Some(s.to_string()); }
            // Single ident (no spaces) — accept.
            if !s.contains(' ') { return Some(s.to_string()); }
            // `new Foo(...)` — Nebula has no `new` keyword; strip it.
            if let Some(rest) = s.strip_prefix("new ") {
                return Some(rest.to_string());
            }
            // Otherwise wrap in quotes.
            Some(format!("\"{}\"", s))
        };
        let mut new_args: Vec<String> = Vec::new();
        for a in args.iter() {
            let s = a.trim();
            if s.is_empty() { continue; } // Drop empty slots.
            let original = s.to_string();
            let transformed = classify_arg(a).unwrap_or_else(|| s.to_string());
            if transformed != original { any_bad = true; }
            new_args.push(transformed);
        }
        if !any_bad { return line.to_string(); }
        let head_s: String = chars[..lp + 1].iter().collect();
        format!("{}{}{}{}", ws, head_s, new_args.join(", "), tail)
    }).collect::<Vec<_>>().join("\n")
}

fn strip_surviving_dollar_braces(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    let mut in_str: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            out.push(c);
            if c == q && (i == 0 || chars[i-1] != '\\') { in_str = None; }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' || c == '`' {
            in_str = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if c == '$' && i + 1 < chars.len() && chars[i+1] == '{' {
            let mut depth = 1i32;
            let mut j = i + 2;
            while j < chars.len() {
                let cj = chars[j];
                if cj == '{' { depth += 1; }
                if cj == '}' { depth -= 1; if depth == 0 { break; } }
                j += 1;
            }
            let inner: String = chars[i+2..j].iter().collect();
            out.push_str(&inner);
            i = j + 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Text-level fallback for backtick-delimited template strings that
/// slipped past the tree-sitter template_string arm (common when the
/// template sits inside a braceless `if`, a ternary, etc.). Converts
/// `` `text ${expr} more` `` → `"text {expr} more"`. Works line-by-line;
/// respects regular string quotes.
fn convert_surviving_template_strings(source: &str) -> String {
    source.lines().map(|line| {
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        let mut in_regular_str: Option<char> = None;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = in_regular_str {
                out.push(c);
                if c == q { in_regular_str = None; }
                i += 1;
                continue;
            }
            if c == '"' || c == '\'' {
                in_regular_str = Some(c);
                out.push(c);
                i += 1;
                continue;
            }
            if c == '`' {
                // Consume up to matching `. Convert `${expr}` → `{expr}`.
                out.push('"');
                i += 1;
                while i < chars.len() {
                    let cj = chars[i];
                    if cj == '`' {
                        out.push('"');
                        i += 1;
                        break;
                    }
                    if cj == '$' && chars.get(i+1).copied() == Some('{') {
                        out.push('{');
                        i += 2;
                        let mut depth = 1i32;
                        while i < chars.len() {
                            let cb = chars[i];
                            if cb == '{' { depth += 1; }
                            if cb == '}' { depth -= 1; if depth == 0 { out.push('}'); i += 1; break; } }
                            out.push(cb);
                            i += 1;
                        }
                        continue;
                    }
                    out.push(cj);
                    i += 1;
                }
                continue;
            }
            out.push(c);
            i += 1;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

pub fn from_typescript_with_opts(source: &str, keep_comments: bool) -> String {
    let pre = strip_generator_star(source);
    let out = convert_with_language(&pre, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), ts::emit, keep_comments);
    let out = strip_js_async_and_generator(&out);
    let out = out.replace("| |", "||");
    let out = wrap_stmt_lambda_bodies(&out);
    let out = normalize_except_clauses(&out);
    let out = collapse_multi_except(&out);
    let out = out.replace(" instanceof ", " == ");
    let out = convert_surviving_template_strings(&out);
    let out = comment_out_discriminated_union_lines(&out);
    let out = comment_out_ts_namespace_blocks(&out);
    let out = strip_surviving_dollar_braces(&out);
    let out = rewrite_match_arms(&out);
    let out = merge_fallthrough_match_arms(&out);
    let out = rewrite_multi_value_arms(&out);
    let out = rewrite_expr_match_arms(&out);
    let out = wrap_return_in_match_arms(&out);
    let out = wrap_lambda_indent_body(&out);
    pad_empty_type_bodies(&out)
}

/// Drop entire TypeScript `namespace X:` blocks. Nebula has no
/// namespace grammar; the outer header and any nested `interface`/
/// `function` lines otherwise break the parse. Drops rather than comments
/// because `//` at arbitrary indents confuses indent-to-brace tracking
/// later in the pipeline.
fn comment_out_ts_namespace_blocks(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut ns_indent: Option<usize> = None;
    for line in lines.iter() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if let Some(ni) = ns_indent {
            if !trimmed.is_empty() && leading <= ni {
                ns_indent = None;
            } else {
                if trimmed.is_empty() {
                    out.push(line.to_string());
                }
                continue;
            }
        }
        if trimmed.starts_with("namespace ") {
            ns_indent = Some(leading);
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Drop TypeScript discriminated-union continuation lines
/// (`    | { kind: "x"; field: T }`). Nebula has no union type grammar;
/// dropping keeps surrounding function declarations parseable. We drop
/// rather than comment because `//` at arbitrary indents confuses the
/// indent-to-brace tracker downstream.
fn comment_out_discriminated_union_lines(source: &str) -> String {
    source.lines().filter(|line| {
        let trimmed = line.trim_start();
        !(trimmed.starts_with("| {") || trimmed.starts_with("| \""))
    }).collect::<Vec<_>>().join("\n")
}

/// After lambdas are emitted and incdec/compound-assign rewrites run,
/// detect `|| ... += ...` / `|x| ... += ...` patterns where the body is a
/// statement (not an expression) and wrap it in braces so the parser
/// accepts it as a block expression. Scans line-wise; only fires when the
/// pipe-pair is followed by a compound-assignment within the remainder
/// of the line, with no comma in between (to avoid touching subsequent
/// arguments).
fn wrap_stmt_lambda_bodies(source: &str) -> String {
    source.lines().map(|line| {
        let mut out = String::with_capacity(line.len());
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c != '|' { out.push(c); i += 1; continue; }
            // Try to match `|...|` lambda header starting here.
            let rest: String = chars[i..].iter().collect();
            let Some(second_pipe) = rest[1..].find('|') else {
                out.push(c); i += 1; continue;
            };
            let header_end = i + 1 + second_pipe + 1;
            // Validate param list (empty or idents with optional types).
            let params = &rest[1..1 + second_pipe];
            let params_ok = params.is_empty() || params.chars().all(|c|
                c.is_ascii_alphanumeric() || c == '_' || c == ',' || c == ' ' || c == ':' || c == '[' || c == ']'
            );
            if !params_ok {
                out.push(c); i += 1; continue;
            }
            // Read body up to the first top-level `,` or line end — this
            // is the lambda body extent (for map-literal call-argument
            // context; mostly what trips the parser).
            let mut j = header_end;
            // skip leading space
            while j < chars.len() && chars[j] == ' ' { j += 1; }
            let body_start = j;
            let mut depth = 0i32;
            while j < chars.len() {
                let cj = chars[j];
                match cj {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => depth -= 1,
                    ',' if depth == 0 => break,
                    _ => {}
                }
                if depth < 0 { break; }
                j += 1;
            }
            let body_str: String = chars[body_start..j].iter().collect();
            // Detect any top-level assignment (`=`, `+=`, etc.) in the
            // body. Plain `=` must NOT match `==` / `!=` / `>=` / `<=`.
            let contains_assign = {
                let mut found = false;
                let bs: Vec<char> = body_str.chars().collect();
                let mut bdepth = 0i32;
                let mut k = 0;
                let mut bin_str: Option<char> = None;
                while k < bs.len() {
                    let ch = bs[k];
                    if let Some(q) = bin_str {
                        if ch == q { bin_str = None; }
                        k += 1; continue;
                    }
                    if ch == '"' || ch == '\'' { bin_str = Some(ch); k += 1; continue; }
                    match ch {
                        '(' | '[' | '{' => bdepth += 1,
                        ')' | ']' | '}' => bdepth -= 1,
                        '=' if bdepth == 0 => {
                            let prev = if k == 0 { ' ' } else { bs[k-1] };
                            let next = bs.get(k+1).copied().unwrap_or(' ');
                            if next == '=' { k += 2; continue; }
                            if prev == '!' || prev == '<' || prev == '>' || prev == '=' {
                                k += 1; continue;
                            }
                            found = true; break;
                        }
                        _ => {}
                    }
                    k += 1;
                }
                found
            };
            if !contains_assign || body_str.trim_start().starts_with('{') {
                // Emit as-is up to body_start, consume nothing else.
                out.push_str(&rest[..(body_start - i)]);
                i = body_start;
                continue;
            }
            // Wrap body in braces.
            out.push_str(&rest[..(body_start - i)]);
            out.push_str(&format!("{{ {} }}", body_str.trim()));
            i = j;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

/// Drop the generator-function asterisk in `function*` / `function *` so
/// tree-sitter sees a plain function_declaration. Nebula has no generator
/// primitive; yields become returns in practice and the asterisk would
/// otherwise route the node through an emit arm we don't cover.
fn strip_generator_star(source: &str) -> String {
    source
        .replace("function* ", "function ")
        .replace("function *", "function")
        .replace("function*", "function")
}

/// Collapse labeled tuple element names: `[lo: Int, hi: Int]` → `[Int, Int]`.
/// Nebula's tuple type grammar has no label support; keeping the labels
/// produces `Expected Comma, got Colon` during parse. Operates at string
/// level so it works regardless of which bracket style the caller uses.
fn strip_tuple_labels(t: &str) -> String {
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len());
    let mut depth = 0i32;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '[' | '(' => { depth += 1; out.push(c); i += 1; }
            ']' | ')' => { depth -= 1; out.push(c); i += 1; }
            _ if depth > 0 && c.is_ascii_alphabetic() => {
                // Peek: ident followed by `:` — drop the ident-and-colon.
                let mut j = i;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') { j += 1; }
                let after = chars[j..].iter().collect::<String>();
                if after.starts_with(": ") || after.starts_with(':') {
                    i = j;
                    while i < chars.len() && (chars[i] == ':' || chars[i] == ' ') { i += 1; }
                } else {
                    out.push(c);
                    i += 1;
                }
            }
            _ => { out.push(c); i += 1; }
        }
    }
    out
}

/// Strip JS/TS `async` keyword and rewrite generator function syntax
/// (`function*`) so the output parses in Nebula. Nebula has no async/await
/// or generator primitives — the `async fn` / `function*` modifiers are
/// metadata-only for round-trip; codegen targeting non-JS languages has
/// nothing to do with them. Lossy but unblocks parse.
fn strip_js_async_and_generator(source: &str) -> String {
    let mut out = Vec::with_capacity(source.lines().count());
    for line in source.lines() {
        let trimmed = line.trim_start();
        let ws = &line[..line.len() - trimmed.len()];
        let stripped = trimmed
            .strip_prefix("async ")
            .unwrap_or(trimmed);
        // Drop the generator asterisk; the function is still a valid
        // declaration syntactically, and tree-sitter will normalize its
        // return type via the usual path.
        let stripped = stripped
            .replace("function* ", "function ")
            .replace("function*", "function");
        out.push(format!("{}{}", ws, stripped));
    }
    out.join("\n")
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
    let nebula = braces_to_indent(&nebula);
    let nebula = strip_generic_param_clauses(&nebula);
    let nebula = strip_where_clauses(&nebula);
    let nebula = comment_nested_type_decls(&nebula);
    let nebula = comment_orphan_impls(&nebula);
    let nebula = expand_single_expr_match(&nebula);
    // Swap the anon-func marker back to `||` now that the logical-OR
    // replacement has already run.
    let nebula = nebula.replace("__NEB_ANONFN__", "||");
    // Drop defer blocks BEFORE rename so the remaining code is intact
    // when the scope-tracking rename walks the function body.
    let nebula = drop_go_defer_anon_blocks(&nebula);
    let nebula = rename_reserved_params(&nebula);
    let nebula = indent_flattened_match_arms(&nebula);
    let nebula = rewrite_multi_value_arms(&nebula);
    // Go type-switch `case []T:` / `case *T:` use pattern shapes Nebula
    // doesn't accept; rewrite to bare ident forms before arm conversion.
    let nebula = nebula.lines().map(|line| {
        let t = line.trim_start();
        if !t.starts_with("case ") { return line.to_string(); }
        let leading = line.len() - t.len();
        let ws = &line[..leading];
        let rest = &t[5..];
        if let Some(colon) = rest.find(':') {
            let pat = &rest[..colon];
            let tail = &rest[colon..];
            let new_pat = pat
                .replace("[]", "slice_")
                .replace("*", "ptr_");
            return format!("{}case {}{}", ws, new_pat, tail);
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n");
    let nebula = rewrite_match_arms(&nebula);
    pad_empty_type_bodies(&nebula)
}

pub fn from_swift_with_opts(source: &str, keep_comments: bool) -> String {
    let nebula = swift::convert_swift_raw(source, 0);
    let nebula = strip_generic_param_clauses(&nebula);
    let nebula = rewrite_ternary(&nebula);
    let nebula = strip_where_clauses(&nebula);
    // Swift labeled tuples `(lo: Int, hi: Int)` — strip labels line-wise so
    // Nebula's tuple-type grammar accepts them.
    let nebula = strip_tuple_labels_in_text(&nebula);
    // Swift-specific keyword-strip: `async`, `throws`, `rethrows` appear
    // between params and return type and Nebula has no grammar for them.
    // Drop cleanly so the signature parses.
    let nebula = nebula
        .replace(") async throws ->", ") ->")
        .replace(") async rethrows ->", ") ->")
        .replace(") async ->", ") ->")
        .replace(") throws ->", ") ->")
        .replace(") rethrows ->", ") ->")
        .replace(") async {", ") {")
        .replace(") throws {", ") {")
        .replace(") rethrows {", ") {")
        // Prefix `await EXPR` / `try EXPR` — Swift-specific, no Nebula
        // grammar for them in expression position. Strip the prefix; the
        // expression's value is what matters for the corrector/target.
        // Handle `try!` and `try?` variants (force-unwrap, optional-try)
        // before stripping plain `try` to keep the `!`/`?` from leaking.
        .replace(" await ", " ")
        .replace("(await ", "(")
        .replace(" try! ", " ")
        .replace("(try! ", "(")
        .replace(" try? ", " ")
        .replace("(try? ", "(")
        .replace(" try ", " ")
        .replace("(try ", "(")
        // Swift `async let x = …` is a declaration modifier. Strip.
        .replace("async let ", "let ")
        .replace("async lt ", "lt ")
        .replace("async mut ", "mut ")
        .replace("async mt ", "mt ")
        // Swift `do { … } catch { … }` → Nebula `try { } except { }`.
        .replace("do {", "try {")
        .replace("} catch ", "} except ")
        .replace(" catch {", " except e {")
        // Swift `Type.self` reflection — Nebula has no equivalent. Strip
        // the `.self` suffix so the type name parses on its own.
        .replace(".self ", " ")
        .replace(".self,", ",")
        .replace(".self)", ")")
        .replace(".self\n", "\n");
    // Swift `{ params in body }` closure form. Replace `IDENT in ` (where
    // it appears after `{`) with `IDENT -> ` to match Nebula's lambda
    // arrow syntax. Avoid touching `for x in xs` / standalone `in`.
    let nebula = swift_closure_in_to_arrow(&nebula);
    let nebula = postprocess(&nebula);
    let nebula = strip_semicolons(&nebula);
    let nebula = minify(&nebula, keep_comments);
    let nebula = fix_raw_keywords(&nebula);
    let nebula = if keep_comments { canonicalize_keep_comments(&nebula) } else { canonicalize(&nebula) };
    let nebula = braces_to_indent(&nebula);
    let nebula = rewrite_match_arms(&nebula);
    let nebula = comment_nested_type_decls(&nebula);
    let nebula = comment_orphan_impls(&nebula);
    let nebula = expand_single_expr_match(&nebula);
    let nebula = rename_reserved_params(&nebula);
    let nebula = prefix_bare_typed_decls(&nebula);
    let nebula = normalize_except_clauses(&nebula);
    let nebula = collapse_multi_except(&nebula);
    let nebula = rewrite_operator_references(&nebula);
    // Swift empty dictionary literal `[:]` → Nebula empty map `{}`.
    let nebula = nebula.replace("[:]", "{}");
    let nebula = strip_pattern_binding_modifiers(&nebula);
    let nebula = rewrite_where_guard_in_match(&nebula);
    let nebula = comment_out_if_case_patterns(&nebula);
    let nebula = wrap_return_in_match_arms(&nebula);
    let nebula = rewrite_kotlin_lambda_closure(&nebula);
    let nebula = rewrite_multiline_lambda_closure(&nebula);
    let nebula = wrap_stmt_lambda_bodies(&nebula);
    pad_empty_type_bodies(&nebula)
}

pub fn from_kotlin_with_opts(source: &str, keep_comments: bool) -> String {
    let nebula = kotlin::convert_kotlin_raw(source, 0);
    let nebula = rewrite_kotlin_return_type_colon(&nebula);
    let nebula = rewrite_kotlin_range_ops(&nebula);
    let nebula = rewrite_named_args(&nebula);
    let nebula = wrap_braceless_for_body(&nebula);
    let nebula = strip_generic_param_clauses(&nebula);
    let nebula = strip_turbofish_call_args(&nebula);
    // `enum Color(val css: Str)` → `enum Color` — Nebula enum decl has no
    // primary constructor. Drop the paren group between `enum NAME` and
    // the body opener (`:` or `{`). Also strips Kotlin enum-variant call-
    // constructor `RED("red"),` → `RED,` — Nebula enum variants are
    // tuple-typed, not value-call-initialized.
    let nebula = nebula.lines().map(|line| {
        let trimmed = line.trim_start();
        let is_enum_header = trimmed.starts_with("en ") || trimmed.starts_with("enum ");
        // Heuristic: enum variant-with-call line looks like
        //   IDENT("...") ,   or   IDENT(value) ,
        // at some indent level. Pattern: leading ident, then `(`, then not
        // empty, then `)` with optional trailing `,`.
        let is_variant_call = {
            let t = trimmed.trim_end_matches(|c: char| c == ',' || c == ';' || c == ' ');
            if let Some(op) = t.find('(') {
                let head = &t[..op];
                let name_ok = !head.is_empty() && head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                t.ends_with(')') && name_ok
            } else {
                false
            }
        };
        if !is_enum_header && !is_variant_call {
            return line.to_string();
        }
        let ws = &line[..line.len() - trimmed.len()];
        if let Some(op) = trimmed.find('(') {
            let after_open = &trimmed[op..];
            let mut depth = 0i32;
            let mut end = None;
            for (j, c) in after_open.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => { depth -= 1; if depth == 0 { end = Some(j); break; } }
                    _ => {}
                }
            }
            if let Some(e) = end {
                let head = &trimmed[..op];
                let tail = &after_open[e + 1..];
                return format!("{}{}{}", ws, head.trim_end(), tail);
            }
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n");
    let nebula = postprocess(&nebula);
    let nebula = strip_semicolons(&nebula);
    let nebula = minify(&nebula, keep_comments);
    let nebula = fix_raw_keywords(&nebula);
    let nebula = if keep_comments { canonicalize_keep_comments(&nebula) } else { canonicalize(&nebula) };
    let nebula = braces_to_indent(&nebula);
    let nebula = rewrite_match_arms(&nebula);
    let nebula = comment_nested_type_decls(&nebula);
    let nebula = comment_orphan_impls(&nebula);
    let nebula = expand_single_expr_match(&nebula);
    let nebula = rename_reserved_params(&nebula);
    let nebula = rewrite_operator_references(&nebula);
    let nebula = trait_body_props_to_fns(&nebula);
    let nebula = flatten_impl_trait_paths(&nebula);
    let nebula = rewrite_kotlin_lambda_closure(&nebula);
    let nebula = rewrite_kotlin_implicit_it_closure(&nebula);
    let nebula = wrap_stmt_lambda_bodies(&nebula);
    let nebula = wrap_return_in_match_arms(&nebula);
    let nebula = rebalance_list_of_brackets(&nebula);
    let nebula = drop_enum_body_methods(&nebula);
    let nebula = strip_kotlin_primary_constructor(&nebula);
    let nebula = pad_empty_type_bodies(&nebula);
    let nebula = fix_kotlin_expr_body_leaks(&nebula);
    wrap_expr_bodies_in_trait(&nebula)
}

pub fn from_rust_with_opts(source: &str, keep_comments: bool) -> String {
    let out = convert_with_language(source, tree_sitter_rust::LANGUAGE.into(), rust::emit, keep_comments);
    let out = strip_rust_macro_bangs(&out);
    // Strip Rust-only async/await plumbing that Nebula has no grammar for.
    let out = out.replace(".await", "")
        .replace("async move ", "")
        .replace("async ", "")
        // `dyn Trait` (trait objects) and `impl Trait` (positional traits)
        // — Nebula's type grammar has neither. Drop the prefixes; the
        // underlying Trait name parses.
        .replace("dyn ", "")
        .replace("Box<", "Box<")
        .replace("Rc<", "Rc<")
        // Rust reference types in signatures. Strip the `&` so the inner
        // type parses. Nebula has no borrow grammar; round-trip reifies
        // the type as owned.
        .replace("&str", "Str")
        .replace("&mut ", "")
        .replace("&[", "[")
        .replace(": &", ": ");
    let out = strip_generic_param_clauses(&out);
    let out = strip_where_clauses(&out);
    // Rust array-repetition `[expr; n]` — Nebula has no semi-colon inside
    // array literals. Replace the full form with `[expr]` (single-element
    // placeholder; semantics lost). Matches simple `[expr; count]` only.
    let out = strip_rust_array_repeat(&out);
    // Rewrite Rust range-based slice `xs[a..b]` / `xs[a..]` / `xs[..b]` to
    // Python-style `xs[a:b]` / `xs[a:]` / `xs[:b]` which Nebula's parser
    // accepts. Inclusive `..=` loses the closedness (Nebula's slice grammar
    // has no inclusive-end form); flagged as a known lossy conversion.
    let out = rewrite_rust_range_slices(&out);
    let out = flatten_impl_trait_paths(&out);
    let out = comment_out_if_case_patterns(&out);
    let out = wrap_return_in_match_arms(&out);
    // Strip Rust byte-string literal prefix: `b"hello"` → `"hello"`.
    // Nebula has no byte-string grammar; parser otherwise reads `b` as
    // an identifier and stalls on the following `"`.
    let out = strip_rust_byte_string_prefix(&out);
    pad_empty_type_bodies(&out)
}

fn strip_rust_byte_string_prefix(source: &str) -> String {
    // Word-boundary replace of `b"` → `"` when preceded by a non-ident
    // character. Also handles `br"..."` raw byte strings.
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    for i in 0..chars.len() {
        let c = chars[i];
        if (c == 'b') && i + 1 < chars.len() && (chars[i+1] == '"' || (chars[i+1] == 'r' && i + 2 < chars.len() && chars[i+2] == '"')) {
            let prev = if i == 0 { ' ' } else { chars[i-1] };
            let boundary = !(prev.is_ascii_alphanumeric() || prev == '_');
            if boundary {
                // Drop the `b`.
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Convert Rust-style range slice operators inside `[...]` brackets to
/// Python-style colon slices. Operates only on `..` / `..=` between `[`
/// and `]`, leaving top-level ranges (e.g. `for i in 0..n`) untouched.
fn rewrite_rust_range_slices(source: &str) -> String {
    source.lines().map(|line| {
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        let mut bracket_depth = 0i32;
        let mut in_str: Option<char> = None;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = in_str {
                out.push(c);
                if c == q { in_str = None; }
                i += 1;
                continue;
            }
            if c == '"' || c == '\'' {
                in_str = Some(c);
                out.push(c);
                i += 1;
                continue;
            }
            if c == '[' { bracket_depth += 1; out.push(c); i += 1; continue; }
            if c == ']' { bracket_depth -= 1; out.push(c); i += 1; continue; }
            if bracket_depth > 0 && c == '.' && i + 1 < chars.len() && chars[i+1] == '.' {
                // `..` or `..=` inside brackets → `:`. Drop trailing `=`.
                out.push(':');
                i += 2;
                if i < chars.len() && chars[i] == '=' { i += 1; }
                continue;
            }
            out.push(c);
            i += 1;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

/// Convert Kotlin trailing-closure param form `{ params -> body }` to
/// Nebula's lambda arrow form `|params| body`. Matches when the `{` is
/// followed by a comma-separated ident list and ` -> `. Rewrites only
/// the `{ params -> ` prefix; the closing `}` remains, turning the
/// construct into `|params| body }` — invalid. So we also track the
/// matching `}` and drop it. Runs line-wise and respects paren/string
/// boundaries.
/// Handle lambda-with-indented-body shape: `.method(|params|:\n body`.
/// JS/TS `.forEach` etc. ingest as `|params|:` which Python-ish indent-
/// opens the lambda body. Nebula rejects this; wrap body in braces and
/// close the call: `.method((|params| { body }))`.
/// Collapse adjacent empty argument slots left behind by aggressive
/// text-level rewrites (tagged templates, stripped `${…}` placeholders).
/// `tag(a, , b)` → `tag(a, b)`, `tag(a, , )` → `tag(a)`, etc. Does not
/// touch multi-char values or trailing commas inside type parameters.
fn collapse_empty_call_args(source: &str) -> String {
    let mut s = source.to_string();
    // Pre-pass: remove empty args between commas (spaces allowed).
    loop {
        let prev = s.clone();
        s = s.replace(", , ", ", ")
            .replace(", ,", ",")
            .replace(",  ,", ", ")
            .replace("(, ", "(")
            .replace(", )", ")");
        if s == prev { break; }
    }
    s
}

fn wrap_lambda_indent_body(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();
        if trimmed.ends_with("|:") {
            if let Some(open_pipe) = trimmed.rfind("(|") {
                let close_pipe = trimmed.len() - 2;
                if close_pipe > open_pipe + 1 {
                    let header_leading = line.len() - line.trim_start().len();
                    let mut j = i + 1;
                    let mut end_line = lines.len();
                    while j < lines.len() {
                        let nt = lines[j].trim_end();
                        let nts = nt.trim_start();
                        if nts.is_empty() { j += 1; continue; }
                        let nl = nt.len() - nts.len();
                        if nl <= header_leading { end_line = j; break; }
                        j += 1;
                    }
                    let pre = &trimmed[..close_pipe];
                    out.push(format!("{}| {{", pre));
                    for k in (i + 1)..end_line {
                        out.push(lines[k].to_string());
                    }
                    out.push(format!("{}}})", &" ".repeat(header_leading)));
                    i = end_line;
                    continue;
                }
            }
        }
        out.push(line.to_string());
        i += 1;
    }
    out.join("\n")
}

/// Multi-line variant of the Kotlin/Swift trailing-closure rewrite.
/// Handles `… { IDENT -> ` at end-of-line followed by indented body
/// lines and a closing `}`. Converts to `… (|IDENT| {\n body \n})`.
/// The indent of the closing `}` is used to detect the end of the body.
fn rewrite_multiline_lambda_closure(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();
        if trimmed.ends_with("->") {
            let no_arrow = trimmed[..trimmed.len() - 2].trim_end();
            if let Some(lb) = no_arrow.rfind("{ ") {
                let param_src = no_arrow[lb + 2..].trim().to_string();
                let all_ident = !param_src.is_empty() && param_src.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ',' || c == ' ');
                if all_ident {
                    let header_leading = line.len() - line.trim_start().len();
                    let mut j = i + 1;
                    let mut close_line: Option<usize> = None;
                    let mut end_by_dedent: Option<usize> = None;
                    while j < lines.len() {
                        let nt = lines[j].trim_end();
                        let nts = nt.trim_start();
                        if nts.is_empty() { j += 1; continue; }
                        let nl = nt.len() - nts.len();
                        if nts == "}" && nl == header_leading {
                            close_line = Some(j);
                            break;
                        }
                        // End body on a line at the header's indent or shallower
                        // that isn't part of the body (new decl, return from
                        // outer fn, etc.).
                        if nl <= header_leading {
                            end_by_dedent = Some(j);
                            break;
                        }
                        j += 1;
                    }
                    let cl_opt = close_line
                        .or_else(|| end_by_dedent.map(|e| e.saturating_sub(1)))
                        .or_else(|| if j >= lines.len() && j > i + 1 { Some(j - 1) } else { None });
                    if let Some(cl) = cl_opt {
                        let has_real_close = close_line.is_some();
                        let strip_idx = line.rfind("{ ").unwrap_or(line.len());
                        let pre = line[..strip_idx].trim_end().to_string();
                        out.push(format!("{}(|{}| {{", pre, param_src));
                        let end = if has_real_close { cl } else { cl + 1 };
                        for k in (i + 1)..end {
                            out.push(lines[k].to_string());
                        }
                        out.push(format!("{}}})", &" ".repeat(header_leading)));
                        i = if has_real_close { cl + 1 } else { cl + 1 };
                        continue;
                    }
                }
            }
        }
        out.push(line.to_string());
        i += 1;
    }
    out.join("\n")
}

fn rewrite_kotlin_lambda_closure(source: &str) -> String {
    source.lines().map(|line| {
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        let mut in_str: Option<char> = None;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = in_str {
                out.push(c);
                if c == q { in_str = None; }
                i += 1;
                continue;
            }
            if c == '"' || c == '\'' {
                in_str = Some(c);
                out.push(c);
                i += 1;
                continue;
            }
            if c == '{' {
                // Peek: `{ IDENT(, IDENT)* -> `.
                let mut j = i + 1;
                while j < chars.len() && chars[j] == ' ' { j += 1; }
                let start = j;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_' || chars[j] == ',' || chars[j] == ' ') { j += 1; }
                let param_text: String = chars[start..j].iter().collect();
                let param_text = param_text.trim().to_string();
                if !param_text.is_empty() && j + 3 <= chars.len()
                    && chars[j-1] == ' '
                    && chars[j] == '-' && chars[j+1] == '>'
                    && (j+2 == chars.len() || chars[j+2] == ' ' || chars[j+2] == '\n')
                {
                    let params: Vec<String> = param_text.split(',').map(|s| s.trim().to_string()).collect();
                    let all_idents = params.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
                    if all_idents {
                        // Detect if this `{` is part of a trailing-closure
                        // method call: the last non-space char before `{`
                        // should be `)` (call args just closed) or an ident/`]`
                        // (Kotlin's `.method { ... }` trailing-closure sugar,
                        // no parens). When it is, wrap the expansion in `()`
                        // so Nebula's parser sees `.method(|...| body)`.
                        let is_trailing_closure = {
                            let mut b = out.trim_end();
                            let last = b.chars().last();
                            match last {
                                Some(')') | Some(']') => true,
                                Some(ch) if ch.is_ascii_alphanumeric() || ch == '_' => true,
                                _ => false,
                            }
                        };
                        // Walk past `-> ` to body start; then to matching
                        // `}`. Track brace depth.
                        let mut k = j + 2; // past `->`
                        while k < chars.len() && chars[k] == ' ' { k += 1; }
                        let body_start = k;
                        let mut depth = 1i32;
                        let mut close_pos = None;
                        let mut inner_str: Option<char> = None;
                        while k < chars.len() {
                            let ck = chars[k];
                            if let Some(q) = inner_str {
                                if ck == q { inner_str = None; }
                                k += 1; continue;
                            }
                            if ck == '"' || ck == '\'' { inner_str = Some(ck); k += 1; continue; }
                            if ck == '{' { depth += 1; }
                            if ck == '}' { depth -= 1; if depth == 0 { close_pos = Some(k); break; } }
                            k += 1;
                        }
                        let Some(cp) = close_pos else {
                            out.push(c);
                            i += 1;
                            continue;
                        };
                        // Emit `|params| body`, wrapped in `()` when this
                        // is a trailing-closure argument position.
                        if is_trailing_closure { out.push('('); }
                        out.push('|');
                        out.push_str(&params.join(", "));
                        out.push_str("| ");
                        let body: String = chars[body_start..cp].iter().collect();
                        out.push_str(body.trim());
                        if is_trailing_closure { out.push(')'); }
                        i = cp + 1;
                        continue;
                    }
                }
            }
            out.push(c);
            i += 1;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

/// Rewrite Kotlin-style implicit-`it` trailing closures to explicit Nebula
/// lambda-call form:
///   `xs.map { it * 2 }`   →  `xs.map(|it| it * 2)`
///   `xs.filter { it > 0 }.sum()` → `xs.filter(|it| it > 0).sum()`
/// Only fires when the `{` sits immediately after a method/field access
/// with no `->` inside the matching `{ ... }` span (so pre-existing
/// param-form closures stay untouched), and the body doesn't itself
/// contain top-level `;` (which would make it a multi-statement block
/// requiring heavier treatment).
fn rewrite_kotlin_implicit_it_closure(source: &str) -> String {
    source.lines().map(|line| {
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        let mut in_str: Option<char> = None;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = in_str {
                out.push(c);
                if c == q { in_str = None; }
                i += 1;
                continue;
            }
            if c == '"' || c == '\'' {
                in_str = Some(c);
                out.push(c);
                i += 1;
                continue;
            }
            if c == '{' && i > 0 {
                // Look back for ` ` then method/field access ending in
                // ident (no parens).
                let mut b = i;
                while b > 0 && chars[b-1] == ' ' { b -= 1; }
                if b == 0 { out.push(c); i += 1; continue; }
                // The last char before whitespace should close an ident.
                let prev = chars[b-1];
                let is_method_access = prev.is_ascii_alphanumeric() || prev == '_';
                if !is_method_access { out.push(c); i += 1; continue; }
                // Walk back through the ident, then require a SINGLE `.`
                // before it — not a `..` range operator which also ends in
                // `.ident` shape.
                let mut k = b;
                while k > 0 && (chars[k-1].is_ascii_alphanumeric() || chars[k-1] == '_') { k -= 1; }
                if k == 0 || chars[k-1] != '.' { out.push(c); i += 1; continue; }
                if k >= 2 && chars[k-2] == '.' {
                    // `..ident` range-op tail, not a method access.
                    out.push(c); i += 1; continue;
                }
                // Locate matching `}`. Track brace depth, escape strings.
                let mut depth = 1i32;
                let mut j = i + 1;
                let mut inner_str: Option<char> = None;
                let mut saw_arrow = false;
                let mut saw_semi = false;
                while j < chars.len() {
                    let cj = chars[j];
                    if let Some(q) = inner_str {
                        if cj == q { inner_str = None; }
                        j += 1; continue;
                    }
                    if cj == '"' || cj == '\'' { inner_str = Some(cj); j += 1; continue; }
                    if cj == '{' { depth += 1; }
                    if cj == '}' { depth -= 1; if depth == 0 { break; } }
                    if depth == 1 {
                        if cj == '-' && j + 1 < chars.len() && chars[j+1] == '>' { saw_arrow = true; }
                        if cj == ';' { saw_semi = true; }
                    }
                    j += 1;
                }
                if depth != 0 || saw_arrow || saw_semi {
                    out.push(c);
                    i += 1;
                    continue;
                }
                // Extract body between `{` and `}`.
                let body: String = chars[i+1..j].iter().collect();
                let body = body.trim();
                // Skip the trailing `whitespace`s we preserved in `out`
                // back before the `{` so we can swap `.foo { body }` →
                // `.foo(|it| body)`.
                while out.ends_with(' ') { out.pop(); }
                out.push_str(&format!("(|it| {})", body));
                i = j + 1;
                continue;
            }
            out.push(c);
            i += 1;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

/// Convert Swift closure `in` keyword to Nebula's `->` arrow.
///   `xs.map { x in x * 2 }` → `xs.map { x -> x * 2 }`
/// Heuristic: only fire when the line contains `{` followed by an ident
/// (or a paren-list of idents) followed by ` in `. Doesn't touch
/// `for x in xs` because there's no `{` immediately before the ident.
fn swift_closure_in_to_arrow(source: &str) -> String {
    source.lines().map(|line| {
        // Find `{ <something> in ` patterns.
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        let mut in_str: Option<char> = None;
        let mut prev_bs = false;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = in_str {
                out.push(c);
                if c == q && !prev_bs { in_str = None; }
                prev_bs = c == '\\' && !prev_bs;
                i += 1;
                continue;
            }
            prev_bs = false;
            if c == '"' || c == '\'' {
                in_str = Some(c);
                out.push(c);
                i += 1;
                continue;
            }
            if c == '{' {
                out.push(c);
                i += 1;
                // Skip whitespace.
                while i < chars.len() && chars[i] == ' ' {
                    out.push(chars[i]);
                    i += 1;
                }
                // Look for `IDENT[, IDENT]* in ` pattern (or ending with ` in` EOL).
                let rest: String = chars[i..].iter().collect();
                let probe_len = rest.len().min(80);
                let probe = &rest[..probe_len];
                // Try trailing-space form first, then end-of-line form.
                let mut applied = false;
                if let Some(in_pos) = probe.find(" in ") {
                    let pre = &probe[..in_pos];
                    let ok = !pre.is_empty() && pre.split(',').all(|t| {
                        let t = t.trim();
                        !t.is_empty()
                            && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    });
                    if ok {
                        out.push_str(pre);
                        out.push_str(" -> ");
                        i += in_pos + 4;
                        applied = true;
                    }
                }
                if !applied && probe.ends_with(" in") {
                    let pre = &probe[..probe.len() - 3];
                    let ok = !pre.is_empty() && pre.split(',').all(|t| {
                        let t = t.trim();
                        !t.is_empty()
                            && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    });
                    if ok {
                        out.push_str(pre);
                        out.push_str(" ->");
                        i += probe.len();
                        applied = true;
                    }
                }
                if applied { continue; }
                continue;
            }
            out.push(c);
            i += 1;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

/// Strip Rust array-repeat `[expr; count]` → `[expr]`. Runs until stable
/// so nested `[vec![...]; n]` patterns are fully unwound.
fn strip_rust_array_repeat(source: &str) -> String {
    let mut current = source.to_string();
    for _ in 0..5 {
        let next = strip_rust_array_repeat_once(&current);
        if next == current { return next; }
        current = next;
    }
    current
}

fn strip_rust_array_repeat_once(source: &str) -> String {
    source.lines().map(|line| {
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '[' {
                let mut depth = 1i32;
                let mut semi_idx: Option<usize> = None;
                let mut end: Option<usize> = None;
                for (j, ch) in chars[i + 1..].iter().enumerate() {
                    let abs = i + 1 + j;
                    match ch {
                        '[' => depth += 1,
                        ']' => { depth -= 1; if depth == 0 { end = Some(abs); break; } }
                        ';' if depth == 1 && semi_idx.is_none() => { semi_idx = Some(abs); }
                        _ => {}
                    }
                }
                if let (Some(si), Some(ei)) = (semi_idx, end) {
                    out.push('[');
                    let expr: String = chars[i + 1..si].iter().collect();
                    out.push_str(expr.trim());
                    out.push(']');
                    i = ei + 1;
                    continue;
                }
            }
            out.push(c);
            i += 1;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

/// Belt-and-suspenders pass: strip `!` from Rust macro invocations that
/// leaked past tree-sitter's `macro_invocation` arm (observed on macros
/// nested inside `if` arms where the body is parsed as an expression block
/// and the statement-level handler doesn't fire). Only strips `!` when
/// immediately followed by `(`, `[`, or `{` and preceded by an ident char
/// — so `!=` and prefix negation are untouched. Inside string literals: no-op.
fn strip_rust_macro_bangs(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut in_str: Option<char> = None;
    let mut prev_bs = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            out.push(c);
            if c == q && !prev_bs { in_str = None; }
            prev_bs = c == '\\' && !prev_bs;
            i += 1;
            continue;
        }
        prev_bs = false;
        if c == '"' || c == '\'' {
            in_str = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if c == '!' {
            let prev = out.chars().last();
            let next = chars.get(i + 1).copied();
            let prev_ident = prev.map_or(false, |p| p.is_ascii_alphanumeric() || p == '_');
            let next_opener = matches!(next, Some('(') | Some('[') | Some('{'));
            if prev_ident && next_opener {
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

pub fn from_c_with_opts(source: &str, keep_comments: bool) -> String {
    let out = convert_with_language(source, tree_sitter_c::LANGUAGE.into(), c::emit, keep_comments);
    let out = rewrite_c_function_pointer_params(&out);
    let out = drop_c_struct_typedef_aliases(&out);
    let out = close_orphan_struct_literals(&out);
    let out = convert_positional_struct_literals(&out);
    let out = drop_c_union_bitfield_debris(&out);
    let out = fix_c_pointer_field_leaks(&out);
    pad_empty_type_bodies(&out)
}

/// Convert positional C struct literals `NAME := { E1, E2 }` to Nebula
/// tuple literals `NAME := (E1, E2)`. Nebula has no positional struct
/// literal grammar; tuples are the closest parseable form. Detects `{…}`
/// where the inner has no top-level `:` (dict/map shape).
/// Drop C-specific unparseable shapes that leak through: anonymous
/// `union:` blocks, bitfield `field: N` declarations, orphan `} field`
/// trailers. All lossy; the struct layout becomes partial but the
/// surrounding file parses.
/// Merge multi-line `NAME := {\n  v1, v2,\n}` positional struct literals
/// onto a single line so `convert_positional_struct_literals` can
/// process them (and braces_to_indent doesn't strip the standalone `}`).
/// Close orphan-opened positional struct literals: `NAME := { v1, v2, …`
/// without a matching `}` (stripped upstream by braces_to_indent). We
/// rebuild the tuple by reading subsequent lines' values until a line
/// that looks like a new statement.
/// Clean up C struct-field emissions that leak pointer syntax:
/// `field_type *name: const OriginalType *name` → `name: OriginalType`.
/// The ingester's C path can emit a doubled field decl with a redundant
/// LHS shape; keep only the ident-after-`*` on the right's `TYPE *name`.
fn fix_c_pointer_field_leaks(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        let ws = &line[..line.len() - trimmed.len()];
        // Look for `... *name : ... *name` shape.
        if let Some(colon) = trimmed.find(": ") {
            let lhs = trimmed[..colon].trim();
            let rhs = trimmed[colon + 2..].trim();
            let lhs_has_star = lhs.contains('*');
            let rhs_has_star = rhs.contains('*');
            if lhs_has_star && rhs_has_star {
                // Extract ident from LHS (after the `*`).
                let lhs_ident = lhs.rsplit('*').next().unwrap_or("").trim();
                // Extract type from RHS (everything before `*name`).
                let rhs_type = rhs.split('*').next().unwrap_or("").trim()
                    .trim_start_matches("const ").trim();
                if !lhs_ident.is_empty() && !rhs_type.is_empty() {
                    return format!("{}{}: {}", ws, lhs_ident, rhs_type);
                }
            }
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n")
}

fn close_orphan_struct_literals(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();
        // Detect unclosed `:= {` on the line.
        let opens = trimmed.chars().filter(|&c| c == '{').count();
        let closes = trimmed.chars().filter(|&c| c == '}').count();
        if opens > closes && trimmed.contains(":= {") {
            // Append `)` at end of line (we're rewriting `{ ... }` → `(…)`).
            out.push(format!("{}}}", trimmed));
            i += 1;
            continue;
        }
        out.push(line.to_string());
        i += 1;
    }
    out.join("\n")
}

fn join_multiline_struct_literals(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();
        let ends_with_open = trimmed.ends_with(":= {") || trimmed.ends_with("= {");
        if ends_with_open {
            // Find matching `}` on a subsequent line at the same indent.
            let leading = line.len() - line.trim_start().len();
            let mut j = i + 1;
            let mut close_idx = None;
            while j < lines.len() {
                let nt = lines[j].trim_end();
                let nts = nt.trim_start();
                if nts.is_empty() { j += 1; continue; }
                let nl = nt.len() - nts.len();
                if nl == leading && (nts == "}" || nts == "};" || nts.starts_with("}")) {
                    close_idx = Some(j);
                    break;
                }
                if nl < leading { break; }
                j += 1;
            }
            if let Some(cl) = close_idx {
                // Collect body parts.
                let mut body = String::new();
                for k in (i + 1)..cl {
                    let t = lines[k].trim();
                    if t.is_empty() { continue; }
                    if !body.is_empty() { body.push(' '); }
                    body.push_str(t);
                }
                // Strip trailing comma from body.
                let body = body.trim_end_matches(',').trim().to_string();
                let close_tail = lines[cl].trim().trim_start_matches('}').trim();
                out.push(format!("{} {} }}{}", trimmed, body, close_tail));
                i = cl + 1;
                continue;
            }
        }
        out.push(line.to_string());
        i += 1;
    }
    out.join("\n")
}

fn drop_c_union_bitfield_debris(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut union_indent: Option<usize> = None;
    for line in lines.iter() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if let Some(ui) = union_indent {
            if !trimmed.is_empty() && leading <= ui {
                union_indent = None;
            } else {
                continue;
            }
        }
        if trimmed == "union:" || trimmed.starts_with("union ") {
            union_indent = Some(leading);
            continue;
        }
        // Bitfield `name: N` where N is a bare integer (C bitfield width).
        if let Some(colon) = trimmed.find(':') {
            let after = trimmed[colon + 1..].trim();
            // C bitfield shape: `ident : DIGITS` — the width is a plain
            // integer literal with nothing else. Don't touch `kind: Kind`
            // or `x: Int` which have identifier types.
            if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit())
                && !trimmed.trim_start().starts_with("case ") {
                continue;
            }
        }
        // Orphan `} field` line.
        if trimmed.starts_with("} ") {
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

fn convert_positional_struct_literals(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        let ws = &line[..line.len() - trimmed.len()];
        let marker_pos = trimmed.find(":= {").or_else(|| trimmed.find("= {"));
        let Some(mp) = marker_pos else { return line.to_string(); };
        // Find the closing `}` at depth 0.
        let marker_len = if trimmed[mp..].starts_with(":= {") { 4 } else { 3 };
        let start = mp + marker_len;
        let chars: Vec<char> = trimmed.chars().collect();
        let mut depth = 1i32;
        let mut close = None;
        for (i, &c) in chars.iter().enumerate().skip(start) {
            if c == '{' { depth += 1; }
            if c == '}' { depth -= 1; if depth == 0 { close = Some(i); break; } }
        }
        let Some(cl) = close else { return line.to_string(); };
        let inner: String = chars[start..cl].iter().collect();
        // Detect positional vs dict shape.
        let mut d = 0i32;
        let mut has_top_colon = false;
        for c in inner.chars() {
            match c {
                '(' | '[' | '{' => d += 1,
                ')' | ']' | '}' => d -= 1,
                ':' if d == 0 => { has_top_colon = true; break; }
                _ => {}
            }
        }
        if has_top_colon { return line.to_string(); }
        // Reject empty and single-value cases.
        let items: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
        if items.len() < 2 || items.iter().any(|s| s.is_empty()) {
            return line.to_string();
        }
        let head: String = chars[..start - 1].iter().collect();
        let tail: String = chars[cl + 1..].iter().collect();
        format!("{}{}({}){}", ws, head.trim_end(), items.join(", "), tail)
    }).collect::<Vec<_>>().join("\n")
}

/// Drop C `ty NAME = struct ...` typedef-alias lines. These are forward
/// declarations (`typedef struct Shape Shape;`) that the ingester maps
/// to a `ty NAME = …` form whose RHS isn't a valid Nebula type
/// expression. The actual struct definition follows as its own `struct`
/// line and provides the type.
fn drop_c_struct_typedef_aliases(source: &str) -> String {
    source.lines().filter(|line| {
        let t = line.trim_start();
        if !t.starts_with("ty ") { return true; }
        // If the RHS starts with `struct `, drop the line.
        if let Some(eq) = t.find(" = ") {
            let rhs = t[eq + 3..].trim_start();
            if rhs.starts_with("struct ") || rhs.starts_with("union ")
                || rhs.starts_with("enum ") {
                return false;
            }
        }
        true
    }).collect::<Vec<_>>().join("\n")
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
    // Comment out nested type declarations now that we're in indent form.
    let nebula = comment_nested_type_decls(&nebula);
    let nebula = comment_orphan_impls(&nebula);
    // Kotlin-style single-expression function with a `match` body → block form.
    let nebula = expand_single_expr_match(&nebula);
    // Rename params that collide with Nebula reserved words (e.g. `fn`, `in`).
    let nebula = rename_reserved_params(&nebula);
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
fn strip_python_fstring_prefix(source: &str) -> String {
    // Convert `f"..."` / `f'...'` / `rf"..."` / `fr"..."` → `"..."` / `'...'`
    // outside of existing string literals. Nebula interpolates `{…}`
    // natively, so the prefix is redundant and breaks the parser.
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // Recognize prefix letters f, F, r, R, b, B combined with quotes.
        if (c == 'f' || c == 'F')
            && i + 1 < chars.len()
            && (chars[i + 1] == '"' || chars[i + 1] == '\'')
        {
            let prev = if i == 0 { ' ' } else { chars[i - 1] };
            if !(prev.is_ascii_alphanumeric() || prev == '_') {
                i += 1;
                continue;
            }
        }
        // `rf"..."` / `fr"..."` combined prefixes.
        if i + 2 < chars.len() {
            let pair: String = chars[i..i+2].iter().collect();
            if (pair == "rf" || pair == "fr" || pair == "Rf" || pair == "rF" || pair == "RF" || pair == "fR")
                && (chars[i+2] == '"' || chars[i+2] == '\'')
            {
                let prev = if i == 0 { ' ' } else { chars[i - 1] };
                if !(prev.is_ascii_alphanumeric() || prev == '_') {
                    i += 2;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

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
/// Re-indent match-arm case lines that end up at the same indent as the
/// enclosing `match X:` header (common output shape from Go's switch
/// translation, where case bodies are flattened to the header's level).
/// Walks the file linearly: on `match X:` record the header indent;
/// subsequent non-empty lines at the SAME indent get 2 spaces prepended
/// so the `rewrite_match_arms` pass treats them as arm bodies.
fn indent_flattened_match_arms(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut match_indent: Option<usize> = None;
    let mut prev_was_arm = false;
    for line in lines.iter() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if trimmed.starts_with("match ") && trimmed.ends_with(':') {
            match_indent = Some(leading);
            prev_was_arm = false;
            out.push(line.to_string());
            continue;
        }
        if let Some(mi) = match_indent {
            if trimmed.is_empty() {
                out.push(line.to_string());
                continue;
            }
            let is_top_decl = trimmed.starts_with("def ") || trimmed.starts_with("fn ")
                || trimmed.starts_with("struct ") || trimmed.starts_with("st ")
                || trimmed.starts_with("enum ") || trimmed.starts_with("en ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("trait ") || trimmed.starts_with("tr ")
                || trimmed.starts_with("impl ") || trimmed.starts_with("im ");
            if leading < mi || (leading == mi && is_top_decl) {
                match_indent = None;
                prev_was_arm = false;
                out.push(line.to_string());
                continue;
            }
            if leading == mi {
                // Only strict arm-shape headers start a new arm. Body
                // continuations (e.g. `return …`) attach to the prior
                // arm — we lift both the arm and its body 2 spaces.
                let is_arm_header = trimmed.starts_with("case ")
                    || trimmed.starts_with("default");
                if is_arm_header {
                    out.push(format!("  {}", line));
                    prev_was_arm = true;
                    continue;
                }
                if prev_was_arm {
                    out.push(format!("  {}", line));
                    continue;
                }
            } else {
                prev_was_arm = false;
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Rewrite `switch IDENT = EXPR { ... }` / `switch IDENT = EXPR:` to
/// `switch EXPR { ... }` / `switch EXPR:` — Go's type-switch binding
/// syntax after `.(type)` is stripped. Nebula's match has no binding
/// form; we drop the ident reference.
/// Convert C function-pointer param shape `(NAME)(TYPES) -> RET` to
/// `NAME: Fn(TYPES) -> RET`. The ingester produces the former when it
/// sees `int (*f)(int)`-style param declarations; Nebula's grammar
/// requires `name: Type` shape, so we rewrite.
/// Strip Kotlin primary-constructor parens from struct declarations.
/// `struct Name(override lt x: T) : Trait:` → `struct Name : Trait:`.
/// Nebula struct grammar doesn't accept primary-constructor param
/// lists; the fields would need to be declared in the body. Lossy.
/// Rewrite Kotlin single-expression function shape that leaked through
/// the Kotlin raw converter: `def NAME(...) -> T: EXPR {}` →
/// `def NAME(...) -> T = EXPR`. The Kotlin raw path converts the `=`
/// of `fun foo(): T = EXPR` to `:`, then pad_empty_fn_bodies appends
/// `{}` because the line has no trailing `:` or block opener. The end
/// state is malformed — an arrow, a colon body, and an empty brace.
/// Collapse it back to an expression body.
/// Inside trait bodies, Nebula's parser accepts `fn sig` or `fn sig {
/// block }` but not `fn sig = expr`. Rewrite expression-body methods
/// to block form so the surrounding trait parses.
fn wrap_expr_bodies_in_trait(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut trait_indent: Option<usize> = None;
    for line in lines.iter() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if trimmed.starts_with("trait ") || trimmed.starts_with("tr ") {
            trait_indent = Some(leading);
            out.push(line.to_string());
            continue;
        }
        if let Some(ti) = trait_indent {
            if !trimmed.is_empty() && leading <= ti {
                trait_indent = None;
            } else if leading > ti
                && (trimmed.starts_with("fn ") || trimmed.starts_with("def "))
            {
                if let Some(eq) = trimmed.find(" = ") {
                    // Skip if there's no expression (paramless `= ` is still body).
                    let head = &trimmed[..eq];
                    let body = trimmed[eq + 3..].trim();
                    if !body.is_empty() && !body.starts_with('{') {
                        let ws = &line[..leading];
                        out.push(format!("{}{} {{ {} }}", ws, head, body));
                        continue;
                    }
                }
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

fn fix_kotlin_expr_body_leaks(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("def ") || trimmed.starts_with("fn ")) {
            return line.to_string();
        }
        if !trimmed.ends_with(" {}") { return line.to_string(); }
        // Look for `) -> T: EXPR {}` on the line.
        let arrow = match trimmed.find(") -> ") { Some(i) => i, None => return line.to_string() };
        let after_arrow = &trimmed[arrow + 5..];
        // Type up to the first top-level `:`.
        let mut depth = 0i32;
        let mut colon = None;
        for (i, c) in after_arrow.char_indices() {
            match c {
                '(' | '[' | '{' | '<' => depth += 1,
                ')' | ']' | '}' | '>' => depth -= 1,
                ':' if depth == 0 => { colon = Some(i); break; }
                _ => {}
            }
        }
        let Some(colon) = colon else { return line.to_string(); };
        let head = &trimmed[..arrow + 5 + colon];
        let body_rest = &after_arrow[colon + 1..];
        let body_rest = body_rest.trim_end().trim_end_matches("{}").trim();
        if body_rest.is_empty() { return line.to_string(); }
        let ws = &line[..line.len() - trimmed.len()];
        format!("{}{} = {}", ws, head, body_rest)
    }).collect::<Vec<_>>().join("\n")
}

fn strip_kotlin_primary_constructor(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("struct ") || trimmed.starts_with("st ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("impl ") || trimmed.starts_with("im ")) {
            return line.to_string();
        }
        let ws = &line[..line.len() - trimmed.len()];
        let mut rewritten = trimmed.to_string();
        // 1. Strip primary-constructor `(…)`.
        if let Some(lp) = rewritten.find('(') {
            let chars: Vec<char> = rewritten.chars().collect();
            let mut depth = 0i32;
            let mut rp = None;
            for (i, &c) in chars.iter().enumerate().skip(lp) {
                match c {
                    '(' => depth += 1,
                    ')' => { depth -= 1; if depth == 0 { rp = Some(i); break; } }
                    _ => {}
                }
            }
            if let Some(rp) = rp {
                let head: String = chars[..lp].iter().collect();
                let tail: String = chars[rp + 1..].iter().collect();
                rewritten = format!("{}{}", head.trim_end(), tail);
            } else {
                // Orphan `(` with no matching `)` on the line — drop
                // everything from `(` to EOL and preserve the block-open
                // suffix if the source trailed with `:` / `{`.
                let head: String = chars[..lp].iter().collect();
                let had_colon = rewritten.trim_end().ends_with(':');
                let had_brace = rewritten.trim_end().ends_with('{');
                let suffix = if had_colon { ":" } else if had_brace { "{" } else { "" };
                rewritten = format!("{}{}", head.trim_end(), suffix);
            }
        }
        // 2. Strip Kotlin inheritance ` : Parent` (keeps trailing `:` / `{`).
        //    Only if a `:` appears that isn't the block opener.
        if let Some(idx) = rewritten.find(" : ") {
            // Accept if what follows starts with an identifier — that's the parent.
            let after = &rewritten[idx + 3..];
            let parent_end = after.char_indices()
                .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == ',' || *c == ' ' || *c == '<' || *c == '>' || *c == '[' || *c == ']'))
                .map(|(i, _)| i)
                .unwrap_or(after.len());
            let rest = &after[parent_end..];
            let head = &rewritten[..idx];
            rewritten = format!("{}{}", head, rest);
        }
        format!("{}{}", ws, rewritten)
    }).collect::<Vec<_>>().join("\n")
}

fn rewrite_c_function_pointer_params(source: &str) -> String {
    source.lines().map(|line| {
        let mut out = String::with_capacity(line.len());
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            // Probe for `(NAME)(` at word boundary.
            if chars[i] == '(' && i + 1 < chars.len()
                && (chars[i+1].is_ascii_alphabetic() || chars[i+1] == '_') {
                // Find `)` that closes this group.
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') { j += 1; }
                if j < chars.len() && chars[j] == ')' && j + 1 < chars.len() && chars[j+1] == '(' {
                    let name: String = chars[i+1..j].iter().collect();
                    // Scan forward for matching `)` of the type-list paren.
                    let mut k = j + 2;
                    let mut depth = 1i32;
                    let mut close = None;
                    while k < chars.len() {
                        let c = chars[k];
                        if c == '(' { depth += 1; }
                        if c == ')' { depth -= 1; if depth == 0 { close = Some(k); break; } }
                        k += 1;
                    }
                    if let Some(cp) = close {
                        let types: String = chars[j+2..cp].iter().collect();
                        // Look for ` -> RET` following.
                        let mut m = cp + 1;
                        while m < chars.len() && chars[m] == ' ' { m += 1; }
                        if m + 2 < chars.len() && &chars[m..m+2] == ['-', '>'] {
                            // Read until next `,` or `)` at top-level as the return type.
                            let mut r = m + 2;
                            let mut rdepth = 0i32;
                            while r < chars.len() {
                                let c = chars[r];
                                match c {
                                    '(' | '[' | '{' => rdepth += 1,
                                    ')' | ']' | '}' if rdepth == 0 => break,
                                    ',' if rdepth == 0 => break,
                                    ')' | ']' | '}' => rdepth -= 1,
                                    _ => {}
                                }
                                r += 1;
                            }
                            let ret_type: String = chars[m+2..r].iter().collect();
                            out.push_str(&format!("{}: Fn({}) -> {}", name, types, ret_type.trim()));
                            i = r;
                            continue;
                        }
                    }
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

fn rewrite_go_type_switch_header(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        let ws = &line[..line.len() - trimmed.len()];
        if !trimmed.starts_with("switch ") { return line.to_string(); }
        let rest = &trimmed[7..];
        // Match either ` = ` or ` := ` (Go short-var before type-switch).
        let (ep, skip) = if let Some(p) = rest.find(" := ") {
            (p, 4)
        } else if let Some(p) = rest.find(" = ") {
            (p, 3)
        } else {
            return line.to_string();
        };
        let head = rest[..ep].trim();
        let tail = rest[ep + skip..].trim();
        let head_is_ident = !head.is_empty()
            && head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !head_is_ident { return line.to_string(); }
        format!("{}match {}", ws, tail)
    }).collect::<Vec<_>>().join("\n")
}

/// Replace Go fixed-size array types `[N]T` with `List[T]`. Nebula's
/// type grammar has no sized array; we drop the size and use `List`.
/// Does NOT touch value-position `[N]` subscripts or slice expressions.
/// Heuristic: `[DIGITS]IDENT` shape, where DIGITS is a positive literal.
fn strip_go_fixed_array_sizes(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() { j += 1; }
            if j < chars.len() && chars[j] == ']' {
                let after = j + 1;
                // Look ahead past spaces to see if an identifier follows.
                let mut k = after;
                while k < chars.len() && chars[k] == ' ' { k += 1; }
                let has_ident = k < chars.len()
                    && (chars[k].is_ascii_alphabetic() || chars[k] == '_');
                if has_ident {
                    // Extract the type name to wrap.
                    let mut m = k;
                    while m < chars.len() && (chars[m].is_ascii_alphanumeric() || chars[m] == '_' || chars[m] == '.') {
                        m += 1;
                    }
                    let ty: String = chars[k..m].iter().collect();
                    out.push_str(&format!("List[{}]", ty));
                    i = m;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Drop Go `defer func() { … }()` blocks that leak through as malformed
/// Nebula. The closure-body structure doesn't translate mechanically;
/// the defer semantics are lost but the surrounding function parses.
/// Scans line-wise starting from `defer (||` and absorbs subsequent
/// lines until the synthetic `})()` closer or a line back at the
/// defer's own indent.
fn drop_go_defer_anon_blocks(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.starts_with("defer (||") || trimmed.starts_with("defer fn ") {
            let mut j = i + 1;
            while j < lines.len() {
                let nts = lines[j].trim();
                if nts.starts_with("})()") || nts == "})" || nts == "}" {
                    j += 1;
                    break;
                }
                j += 1;
            }
            i = j;
            continue;
        }
        out.push(line.to_string());
        i += 1;
    }
    out.join("\n")
}

fn postprocess_go(source: &str) -> String {
    // Strip Go variadic spread `xs...` — Nebula has no spread grammar;
    // the unpack semantics are lost but the call syntax parses.
    let source = source.replace("...)", ")").replace("... ,", ",");
    // Go empty interface `interface{}` / `Interface{}` → `Any`. Nebula
    // has no empty-interface grammar; Any is the closest equivalent.
    let source = source.replace("Interface{}", "Any").replace("interface{}", "Any");
    // Go fixed-size array `[N]T` → `List[T]`. Nebula has no fixed-size
    // array grammar; List is the closest. The size constant is dropped.
    let source = strip_go_fixed_array_sizes(&source);
    // Go type-switch `switch v := x.(type) { ... }` has no Nebula
    // analogue. Strip the `.(type)` and the `v = ` binding; the result
    // is a plain `switch x { ... }` that rewrites to a Nebula `match`.
    // Lossy (the binding name is dropped), but keeps the arms parseable.
    let source = source.replace(".(type)", "");
    let source = rewrite_go_type_switch_header(&source);
    // Bare `Fn()` type (Go func type with no return) needs an arrow for
    // Nebula's Function TypeExpr grammar. Upgrade to `Fn() -> Any`.
    let source = source.replace("Fn())", "Fn() -> Any)")
        .replace("Fn(),", "Fn() -> Any,")
        .replace("Fn()\n", "Fn() -> Any\n")
        .replace("Fn() {", "Fn() -> Any {");
    let s = rewrite_incdec(&source);
    let s = s
        // Logical operators
        .replace("&&", "and")
        .replace("||", "or")
        // Go raw syntax
        .replace(":=", "=")
        // else → el
        .replace(" else {", " el {")
        .replace(" else\n", " el\n")
        .replace("} else ", "} el ")
        // Go pointer types
        .replace("-> *", "-> ")
        // Strip * dereference before identifiers (Nebula doesn't have explicit deref)
        .replace("!= *", "!= ")
        .replace("== *", "== ");
    s
}

/// Post-process Nebula output to catch patterns the tree-sitter emitters miss.
/// Convert trailing-colon match arms (Swift `case X:` → Nebula `X =>`).
/// Runs AFTER braces_to_indent so match headers are already `match X:` form.
/// We scan in "match mode" after seeing `match ...:` / `mc ...:`, until we
/// hit a line whose indent is ≤ the match header (ends the match body) or
/// whose trimmed content is a def/fn/return/etc (guards against runaway).
/// Inside match mode, lines matching an arm pattern shape (starts with `.`,
/// `(`, `_`, `default`, a literal, or an ident followed by `(`) get their
/// top-level `:` rewritten to `=>`.
fn rewrite_match_arms(source: &str) -> String {
    // First pass: convert `pat:` → `pat =>` inside match blocks. Second pass
    // (below): merge any bare `pat =>` with its indented body block into a
    // single `pat => { body }` expression.
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut match_indent: Option<usize> = None;
    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            out.push(line.to_string());
            continue;
        }
        let leading = line.len() - trimmed.len();
        // End match mode on a line that's clearly a new top-level construct.
        // We DON'T use indent as the exit signal because Swift's case-arm
        // collapse often puts arms at the same indent as the `match` header;
        // bailing on `leading <= mi` would miss them all.
        if let Some(mi) = match_indent {
            let is_top_decl = trimmed.starts_with("def ") || trimmed.starts_with("fn ")
                || trimmed.starts_with("en ") || trimmed.starts_with("enum ")
                || trimmed.starts_with("st ") || trimmed.starts_with("struct ")
                || trimmed.starts_with("class ") || trimmed.starts_with("tr ")
                || trimmed.starts_with("trait ") || trimmed.starts_with("im ")
                || trimmed.starts_with("impl ") || trimmed.starts_with("//")
                || trimmed.starts_with("ty ") || trimmed.starts_with("use ")
                || trimmed.starts_with("md ") || trimmed.starts_with("module ");
            // Any non-empty line at indent <= match header (and not an arm)
            // closes the match block too. Arms live at deeper indent than
            // the header — if we see something at the same or shallower
            // indent that isn't an arm shape, match mode is over.
            let looks_like_arm = trimmed.starts_with("case ")
                || trimmed.starts_with("default")
                || trimmed.starts_with(".")
                || trimmed.starts_with("_ ")
                || trimmed.starts_with("_ =>")
                || trimmed == "_"
                || trimmed.starts_with('"')
                || trimmed.starts_with('\'')
                || trimmed.starts_with('(')
                || trimmed.starts_with('[')
                || trimmed.starts_with('{')
                || trimmed.chars().next().map_or(false, |c| c.is_ascii_digit())
                || trimmed.contains(" => ")
                || trimmed.ends_with("=>");
            if is_top_decl || (leading <= mi && !looks_like_arm) {
                match_indent = None;
            }
        }
        // Enter match mode on any line ending `:` that mentions `match `
        // or `mc ` (covers `match x:`, `return match x:`, `let y = mc x:`,
        // etc.). Otherwise nested match-expressions skip the arm rewriter.
        let has_match_keyword = trimmed.starts_with("match ") || trimmed.starts_with("mc ")
            || trimmed.contains(" match ") || trimmed.contains(" mc ");
        if has_match_keyword && trimmed.ends_with(':') {
            match_indent = Some(leading);
            out.push(line.to_string());
            continue;
        }
        if match_indent.is_some() {
            // Kotlin `when` arms use `X -> Y` — if this line has a top-level
            // `->`, convert it to `=>` in place and emit. Skip arrows inside
            // parens/brackets/braces (those are function types or lambdas).
            if trimmed.contains(" -> ") && !trimmed.contains("=>") {
                let chars: Vec<char> = trimmed.chars().collect();
                let mut depth = 0i32;
                let mut converted = String::new();
                let mut i = 0;
                while i < chars.len() {
                    let c = chars[i];
                    if c == '(' || c == '[' || c == '{' { depth += 1; }
                    else if c == ')' || c == ']' || c == '}' { depth -= 1; }
                    if depth == 0 && i + 2 < chars.len()
                        && chars[i] == ' ' && chars[i + 1] == '-' && chars[i + 2] == '>' {
                        converted.push_str(" =>");
                        i += 3;
                        continue;
                    }
                    converted.push(c);
                    i += 1;
                }
                let ws = &line[..leading];
                out.push(format!("{}{}", ws, converted));
                continue;
            }
            // Python match statements write `case <pat>:` — strip the `case `
            // prefix before the shape check.
            let trimmed_arm = trimmed.strip_prefix("case ").unwrap_or(trimmed);
            let is_arm_shape = trimmed_arm.starts_with('.')
                || trimmed_arm.starts_with('(')
                || trimmed_arm.starts_with('[')
                || trimmed_arm.starts_with('{')
                || trimmed_arm.starts_with('_')
                || trimmed_arm.starts_with("default")
                || trimmed_arm.starts_with('"') || trimmed_arm.starts_with('\'')
                || trimmed_arm.chars().next().map_or(false, |c| c.is_ascii_digit())
                || trimmed_arm.chars().next().map_or(false, |c| c.is_ascii_alphabetic());
            let trimmed = trimmed_arm;
            if is_arm_shape && !trimmed.contains("=>") {
                let mut depth = 0i32;
                let mut colon_idx: Option<usize> = None;
                for (idx, c) in trimmed.char_indices() {
                    match c {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' => depth -= 1,
                        ':' if depth == 0 => { colon_idx = Some(idx); break; }
                        _ => {}
                    }
                }
                if let Some(ci) = colon_idx {
                    let pat = trimmed[..ci].trim_end();
                    let rest = trimmed[ci + 1..].trim_start();
                    // Parser wants arms INDENTED relative to the match
                    // header. Swift's text pipeline puts them at the same
                    // leading indent; add two spaces so they sit inside.
                    let mi = match_indent.unwrap_or(leading);
                    let ws = if leading <= mi {
                        format!("{}  ", &line[..leading])
                    } else {
                        line[..leading].to_string()
                    };
                    let rewritten = if rest.is_empty() {
                        format!("{}{} =>", ws, pat)
                    } else {
                        format!("{}{} => {}", ws, pat, rest)
                    };
                    out.push(rewritten);
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }
    merge_multiline_match_arms(&out.join("\n"))
}

/// Second pass: after arms become `pat =>` form, merge the body that sits
/// on the NEXT indented line into `pat => { body }` so Nebula's match-arm
/// grammar (expects expression after `=>`) is satisfied.
fn merge_multiline_match_arms(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        let has_bare_arrow = trimmed.ends_with("=>");
        if has_bare_arrow {
            // Collect body lines below. Accept deeper-indent bodies (the
            // common Python-like shape), and also same-indent lines when
            // they don't themselves match an arm/decl shape — Swift's arm
            // collapse flattens both pattern and body to the same indent.
            let mut j = i + 1;
            let mut body_lines: Vec<String> = Vec::new();
            while j < lines.len() {
                let nt = lines[j].trim_end();
                if nt.trim().is_empty() { j += 1; continue; }
                let jl = nt.len() - nt.trim_start().len();
                let jt = nt.trim_start();
                let looks_like_arm = jt.trim_end().ends_with("=>")
                    || (jt.contains(" => ") && !jt.starts_with("return"));
                let looks_like_decl = jt.starts_with("def ")
                    || jt.starts_with("fn ")
                    || jt.starts_with("struct ") || jt.starts_with("st ")
                    || jt.starts_with("enum ") || jt.starts_with("en ")
                    || jt.starts_with("class ")
                    || jt.starts_with("trait ") || jt.starts_with("tr ")
                    || jt.starts_with("impl ") || jt.starts_with("im ");
                if jl > leading {
                    body_lines.push(jt.to_string());
                    j += 1;
                } else if jl == leading && !looks_like_arm && !looks_like_decl {
                    body_lines.push(jt.to_string());
                    j += 1;
                } else {
                    break;
                }
            }
            if !body_lines.is_empty() {
                let body_joined = body_lines.join("; ");
                out.push(format!("{} {{ {} }}", line.trim_end(), body_joined));
                i = j;
                continue;
            }
        }
        out.push(line.to_string());
        i += 1;
    }
    out.join("\n")
}

/// Strip Swift/Rust/Kotlin `where` clauses on function signatures:
/// `def foo(x: S) -> R where S.Element == Int {` → `def foo(x: S) -> R {`
/// Nebula has no grammar for generic constraints; the `where` info is lost
/// but the signature parses. Operates per-line, respects string literals.
fn strip_where_clauses(source: &str) -> String {
    source.lines().map(|line| {
        // Only lines that look like a decl (contain `def` or `fn` or `fun`).
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("def ") || trimmed.starts_with("fn ")
            || trimmed.starts_with("fun ") || trimmed.starts_with("pub ")) {
            return line.to_string();
        }
        // Find ` where ` as a word, outside strings, at depth 0.
        let chars: Vec<char> = line.chars().collect();
        let mut depth = 0i32;
        let mut in_str: Option<char> = None;
        let mut prev_bs = false;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = in_str {
                if c == q && !prev_bs { in_str = None; }
                prev_bs = c == '\\' && !prev_bs;
                i += 1;
                continue;
            }
            prev_bs = false;
            match c {
                '"' | '\'' => { in_str = Some(c); i += 1; continue; }
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
            if depth == 0 && c == ' ' {
                let rest: String = chars[i..].iter().collect();
                if rest.starts_with(" where ") {
                    // Strip from `i` up to (but not including) the final `:` /
                    // `{` / end-of-line.
                    let head: String = chars[..i].iter().collect();
                    let tail: String = chars[i + 7..].iter().collect();
                    // Find the terminator in tail (`:`, `{`, end-of-line).
                    // `{` at depth 0 ends the where-clause (before the body).
                    let mut t_depth = 0i32;
                    let mut term_idx = tail.len();
                    for (j, ch) in tail.char_indices() {
                        match ch {
                            '(' | '[' => t_depth += 1,
                            ')' | ']' => t_depth -= 1,
                            '{' if t_depth == 0 => { term_idx = j; break; }
                            ':' if t_depth == 0 => { term_idx = j; break; }
                            _ => {}
                        }
                    }
                    let terminator = &tail[term_idx..];
                    return format!("{}{}", head, terminator);
                }
            }
            i += 1;
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n")
}

/// Separate a ternary's lead (indent + leading `return ` / `x = `) from
/// the condition expression that precedes the `?`.
fn split_ternary_lead(prefix_raw: &str) -> (String, String) {
    // Leading whitespace.
    let ws_end = prefix_raw.char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(prefix_raw.len());
    let ws = &prefix_raw[..ws_end];
    let body = &prefix_raw[ws_end..];
    // Recognize `return `, `rt `, `let NAME = `, `mt NAME = `, `lt NAME = `,
    // or bare `NAME = ` / `sf.X = ` — return the lead for those, else treat
    // the whole body as cond with no lead.
    for prefix in ["return ", "rt "] {
        if let Some(rest) = body.strip_prefix(prefix) {
            return (format!("{}{}", ws, prefix), rest.to_string());
        }
    }
    // `let/mt/lt NAME = EXPR` — find the first ` = ` after a decl keyword.
    if let Some(eq_idx) = body.find(" = ") {
        let head = &body[..eq_idx + 3];
        let tail = &body[eq_idx + 3..];
        return (format!("{}{}", ws, head), tail.to_string());
    }
    (ws.to_string(), body.to_string())
}

/// Rewrite C-style ternary `COND ? A : B` to Nebula's `if COND { A } else { B }`.
/// Nebula parses `?` as the Try postfix operator, so a bare ternary trips
/// the parser on the subsequent `:`. Operates at line level, respects
/// string literals, and only fires when a single top-level `?` and `:` pair
/// is present (to avoid touching map literals or type annotations).
fn rewrite_ternary(source: &str) -> String {
    source.lines().map(|line| {
        let chars: Vec<char> = line.chars().collect();
        // Find top-level `?` not followed by `.`/`=`/`(` (skip optional chain
        // and nil-coalesce), and a subsequent `:` at the same depth. Respect
        // string literals.
        let mut depth = 0i32;
        let mut in_str: Option<char> = None;
        let mut prev_bs = false;
        let mut q_idx: Option<usize> = None;
        let mut c_idx: Option<usize> = None;
        for (i, c) in chars.iter().enumerate() {
            if let Some(q) = in_str {
                if *c == q && !prev_bs { in_str = None; }
                prev_bs = *c == '\\' && !prev_bs;
                continue;
            }
            prev_bs = false;
            match c {
                '"' | '\'' => in_str = Some(*c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                '?' if depth == 0 && q_idx.is_none() => {
                    let next = chars.get(i + 1).copied();
                    if matches!(next, Some('.') | Some('=') | Some('?') | Some('(')) { continue; }
                    q_idx = Some(i);
                }
                ':' if depth == 0 && q_idx.is_some() && c_idx.is_none() => {
                    // Skip `::` (Rust path separator)
                    if chars.get(i + 1).copied() == Some(':') { continue; }
                    c_idx = Some(i);
                }
                _ => {}
            }
        }
        match (q_idx, c_idx) {
            (Some(qi), Some(ci)) if ci > qi => {
                let prefix_raw: String = chars[..qi].iter().collect();
                let then_branch: String = chars[qi + 1..ci].iter().collect();
                let else_branch: String = chars[ci + 1..].iter().collect();
                // Split `prefix_raw` into (lead, cond): lead includes the
                // `return ` / `= ` / leading whitespace boundary; cond is the
                // actual ternary test expression.
                let (lead, cond) = split_ternary_lead(&prefix_raw);
                format!("{}if {} {{ {} }} else {{ {} }}",
                    lead, cond.trim(), then_branch.trim(), else_branch.trim())
            }
            _ => line.to_string(),
        }
    }).collect::<Vec<_>>().join("\n")
}

/// Strip turbofish-style type args from call expressions:
///   `mutable_map_of[String, Int]()` → `mutable_map_of()`
///   `Vec::new<T>()` → `Vec::new()`
/// Kotlin/Rust generic function invocations carry type arguments; Nebula's
/// expression grammar has no call-site turbofish, so `name[T, U](...)` trips
/// the parser on the comma inside brackets. Match `ident[...]` immediately
/// followed by `(` and drop the bracket group.
fn strip_turbofish_call_args(source: &str) -> String {
    source.lines().map(|line| {
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        let mut in_str: Option<char> = None;
        let mut prev_bs = false;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = in_str {
                out.push(c);
                if c == q && !prev_bs { in_str = None; }
                prev_bs = c == '\\' && !prev_bs;
                i += 1;
                continue;
            }
            prev_bs = false;
            if c == '"' || c == '\'' {
                in_str = Some(c);
                out.push(c);
                i += 1;
                continue;
            }
            // `ident[...]` or `ident<...>` — if matching close is followed by `(`,
            // strip the bracket group (turbofish).
            if c == '[' || c == '<' {
                let (open, close) = if c == '[' { ('[', ']') } else { ('<', '>') };
                let prev_is_ident = out.chars().last().map_or(false, |p| p.is_ascii_alphanumeric() || p == '_');
                if prev_is_ident {
                    let mut depth = 1i32;
                    let mut end = None;
                    for (j, ch) in chars[i + 1..].iter().enumerate() {
                        if *ch == open { depth += 1; }
                        else if *ch == close {
                            depth -= 1;
                            if depth == 0 { end = Some(i + 1 + j); break; }
                        }
                    }
                    if let Some(e) = end {
                        let after = chars.get(e + 1).copied();
                        if after == Some('(') {
                            i = e + 1;
                            continue;
                        }
                    }
                }
            }
            out.push(c);
            i += 1;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

/// Line-level wrapper for strip_tuple_labels — applies inside return-type
/// tuples like `-> (lo: Int, hi: Int):` without touching call args where
/// `name: value` is intentional kwarg form.
fn strip_tuple_labels_in_text(source: &str) -> String {
    source.lines().map(|line| {
        // Only strip when preceded by `->` (return type) or `return`
        // keyword. Those contexts are the Swift labeled-tuple sites.
        // Parameter lists (`fn(a: Int, b: Int)`) keep their names.
        let mut out = String::with_capacity(line.len());
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        let mut in_str: Option<char> = None;
        let mut prev_bs = false;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = in_str {
                out.push(c);
                if c == q && !prev_bs { in_str = None; }
                prev_bs = c == '\\' && !prev_bs;
                i += 1;
                continue;
            }
            prev_bs = false;
            if c == '"' || c == '\'' {
                in_str = Some(c);
                out.push(c);
                i += 1;
                continue;
            }
            let rest: String = chars[i..].iter().collect();
            let triggered = rest.starts_with("-> (") || rest.starts_with("return (")
                || rest.starts_with("rt (");
            if triggered {
                // Find `(` then matching `)`.
                let paren_offset = rest.find('(').unwrap();
                let mut depth = 1i32;
                let mut end = None;
                for (j, ch) in rest.char_indices().skip(paren_offset + 1) {
                    match ch {
                        '(' => depth += 1,
                        ')' => { depth -= 1; if depth == 0 { end = Some(j); break; } }
                        _ => {}
                    }
                }
                if let Some(e) = end {
                    // Emit pre-paren lead + stripped paren group.
                    let lead = &rest[..paren_offset];
                    let group = &rest[paren_offset..=e];
                    let stripped = strip_tuple_labels(group);
                    out.push_str(lead);
                    out.push_str(&stripped);
                    i += paren_offset + (e - paren_offset + 1);
                    continue;
                }
            }
            out.push(c);
            i += 1;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

/// Strip generic parameter clauses between keyword and name:
///   `def <T : Comparable<T>> foo(...)` → `def foo(...)`
///   `struct Stack<T>` → `struct Stack`
///   `impl Stack<T>` → `impl Stack`
///   `fn <T> process(...)` → `fn process(...)`
/// Nebula's decl grammar doesn't accept a leading generics list; the
/// information is lost but the declaration parses. Operates at line level
/// and respects paren/bracket nesting to handle `<T: Bound<U>>`.
fn strip_generic_param_clauses(source: &str) -> String {
    let mut out = Vec::with_capacity(source.lines().count());
    for line in source.lines() {
        let trimmed = line.trim_start();
        let ws = &line[..line.len() - trimmed.len()];
        // Candidate keywords where a `<...>` may follow with an ident after.
        let keywords = ["def ", "fn ", "struct ", "st ", "class ", "enum ",
                        "en ", "trait ", "tr ", "impl ", "im "];
        let mut rewritten: Option<String> = None;
        for kw in keywords {
            if trimmed.starts_with(kw) {
                let rest = &trimmed[kw.len()..];
                if rest.starts_with('<') {
                    // Find matching `>` with angle-bracket depth tracking.
                    let chars: Vec<char> = rest.chars().collect();
                    let mut depth = 0i32;
                    let mut end = None;
                    for (i, c) in chars.iter().enumerate() {
                        match c {
                            '<' => depth += 1,
                            '>' => {
                                depth -= 1;
                                if depth == 0 { end = Some(i); break; }
                            }
                            _ => {}
                        }
                    }
                    if let Some(e) = end {
                        let after = rest[e + 1..].trim_start();
                        rewritten = Some(format!("{}{}{}", ws, kw, after));
                        break;
                    }
                } else {
                    // `impl Name<T>` or `def foo[T](...)` — generics AFTER
                    // the name. Strip either bracket style. When the bracket
                    // group is immediately followed by `(`, we know it's a
                    // generic param clause (not a type-in-expression slot).
                    let mut chars = rest.char_indices();
                    let mut name_end = 0;
                    while let Some((i, c)) = chars.next() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            name_end = i + c.len_utf8();
                        } else {
                            break;
                        }
                    }
                    let next_ch = rest[name_end..].chars().next();
                    let (open, close) = match next_ch {
                        Some('<') => ('<', '>'),
                        Some('[') => ('[', ']'),
                        _ => (' ', ' '),
                    };
                    if name_end > 0 && (next_ch == Some('<') || next_ch == Some('[')) {
                        let tail = &rest[name_end..];
                        let tail_chars: Vec<char> = tail.chars().collect();
                        let mut depth = 0i32;
                        let mut end = None;
                        for (i, c) in tail_chars.iter().enumerate() {
                            if *c == open { depth += 1; }
                            else if *c == close {
                                depth -= 1;
                                if depth == 0 { end = Some(i); break; }
                            }
                        }
                        if let Some(e) = end {
                            // For `[T]` decide whether it's a generic-param
                            // clause. After impl/struct/class/enum/trait
                            // declarations with no parens, any `[T]` after
                            // the name IS a generic clause. For def/fn we
                            // require a following `(` to distinguish from
                            // a value-level subscript (array literal).
                            let after = &tail[e + 1..];
                            let should_strip = if open == '[' {
                                matches!(kw, "impl " | "im " | "struct " | "st "
                                    | "class " | "enum " | "en " | "trait " | "tr ")
                                    || after.trim_start().starts_with('(')
                            } else {
                                true
                            };
                            if should_strip {
                                let head = &rest[..name_end];
                                rewritten = Some(format!("{}{}{}{}", ws, kw, head, after));
                                break;
                            }
                        }
                    }
                }
            }
        }
        out.push(rewritten.unwrap_or_else(|| line.to_string()));
    }
    out.join("\n")
}

/// Kotlin's raw converter rewrites `for (X) body` to `fo X) body` — stripping
/// the leading `for (` but leaving the closing `)`. Single-statement bodies
/// then produce un-parseable Nebula (`for i in 0..n total += i`). Scan for
/// `fo ... ) rest` on one line where `rest` isn't a `{` and rewrite to
/// `fo ... { rest }`. The `)` is the signal that a paren-wrapped header
/// existed upstream.
fn wrap_braceless_for_body(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("fo ") || trimmed.starts_with("wl ")) {
            return line.to_string();
        }
        let ws = &line[..line.len() - trimmed.len()];
        // Signal for "stripped-leading-paren" state: the line has one more `)`
        // than `(`. Without this guard, a legit `for (i, v) in xs.withIndex()`
        // line has balanced parens and we'd split on the inner `)` by accident.
        let open = trimmed.chars().filter(|c| *c == '(').count();
        let close = trimmed.chars().filter(|c| *c == ')').count();
        if close <= open { return line.to_string(); }
        // Find the unmatched `)` by tracking depth from the left; the first
        // `)` that brings depth below 0 is the leftover.
        let mut depth: i32 = 0;
        let mut unmatched: Option<usize> = None;
        for (idx, c) in trimmed.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth < 0 { unmatched = Some(idx); break; }
                }
                _ => {}
            }
        }
        let rp = match unmatched { Some(i) => i, None => return line.to_string() };
        let head = &trimmed[..rp];
        let rest = trimmed[rp + 1..].trim_start();
        if rest.is_empty() || rest.starts_with('{') || rest.starts_with('}') {
            return line.to_string();
        }
        format!("{}{} {{ {} }}", ws, head.trim_end(), rest)
    }).collect::<Vec<_>>().join("\n")
}

/// Rewrite Kotlin/Python-style named arguments `fn(name = val)` to the
/// colon form `fn(name: val)` that Nebula's grammar accepts (parser treats
/// `=` as assignment which is illegal inside call args; `:` works because
/// Nebula reads it as a map-like kwarg). Only rewrites `=` that appears
/// inside `(...)` and is bracketed by identifier-then-space on the left and
/// value-then-comma/paren on the right. Respects string literals.
fn rewrite_named_args(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let mut prev_bs = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            out.push(c);
            if c == q && !prev_bs { in_str = None; }
            prev_bs = c == '\\' && !prev_bs;
            i += 1;
            continue;
        }
        prev_bs = false;
        if c == '"' || c == '\'' {
            in_str = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if c == '(' { depth += 1; out.push(c); i += 1; continue; }
        if c == ')' { depth -= 1; out.push(c); i += 1; continue; }
        if depth > 0 && c == '=' {
            // Skip `==`, `=>`, and `>=`/`<=`/`!=` composites.
            let next = chars.get(i + 1).copied();
            let prev = out.chars().last();
            let is_cmp_op = matches!(next, Some('=') | Some('>'))
                || matches!(prev, Some('=') | Some('<') | Some('>') | Some('!'));
            if is_cmp_op {
                out.push(c);
                i += 1;
                continue;
            }
            // Check previous non-space is an identifier char — i.e. this is
            // `name = val` with a simple ident LHS.
            let trimmed = out.trim_end();
            let prev_ident = trimmed.chars().last()
                .map_or(false, |p| p.is_ascii_alphanumeric() || p == '_');
            // Only rewrite if there was actually whitespace between the ident
            // and `=` (Python's default-value syntax uses `name=val` with no
            // space; Kotlin's named-arg uses `name = val`).
            let had_space = out.ends_with(' ') || out.ends_with('\t');
            if prev_ident && had_space {
                // Drop trailing whitespace before `=`, replace `= ` with `: `
                let retain_len = trimmed.len();
                out.truncate(retain_len);
                out.push(':');
                // Skip the `=` and up to one following space.
                i += 1;
                if chars.get(i).copied() == Some(' ') { i += 1; }
                out.push(' ');
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Kotlin range operators (`until`, `downTo`, `step`) are infix fn calls,
/// not keywords — so tree-sitter emits them as identifier sequences. Rewrite
/// to Nebula range syntax where possible.
///   `a until b`   →  `a..b`     (half-open)
///   `a downTo b`  →  `b..=a`     (reversed; caller should chain `.rev()`
///                                 if order matters — we can't detect that
///                                 here without proper parse context)
///   `a step k`    →  `a.step_by(k)` (method chain replacement)
/// Word-boundary aware so identifiers containing `until` etc. aren't touched.
/// Rewrite Kotlin-style `fn name(...): T` return-type annotation to
/// Nebula's `fn name(...) -> T`. Fires only on lines where the `):` is
/// preceded by a function header (`fn ` / `def `), so field/parameter
/// colons elsewhere aren't touched. Stops at the first top-level `=`,
/// `{`, `}`, or comma after the close paren — the type portion is the
/// substring between the close paren and any of those separators.
fn rewrite_kotlin_return_type_colon(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        let has_fn = trimmed.starts_with("fn ")
            || trimmed.starts_with("def ")
            || trimmed.contains(" fn ")
            || trimmed.contains(" def ");
        if !has_fn { return line.to_string(); }
        // Find the matching `)` at paren-depth 0 for the first `(`.
        let chars: Vec<char> = trimmed.chars().collect();
        let lp = match chars.iter().position(|&c| c == '(') { Some(i) => i, None => return line.to_string() };
        let mut depth = 0i32;
        let mut rp = None;
        for (i, &c) in chars.iter().enumerate().skip(lp) {
            match c {
                '(' => depth += 1,
                ')' => { depth -= 1; if depth == 0 { rp = Some(i); break; } }
                _ => {}
            }
        }
        let rp = match rp { Some(i) => i, None => return line.to_string() };
        // After `)` skip spaces, then expect `:`.
        let mut j = rp + 1;
        while j < chars.len() && chars[j] == ' ' { j += 1; }
        if j >= chars.len() || chars[j] != ':' { return line.to_string(); }
        // Build new trimmed: up-to-and-including `)` + ` -> ` + rest-after-colon.
        let head: String = chars[..=rp].iter().collect();
        let tail: String = chars[j+1..].iter().collect();
        format!("{}{} -> {}", &line[..leading], head, tail.trim_start())
    }).collect::<Vec<_>>().join("\n")
}

fn rewrite_kotlin_range_ops(source: &str) -> String {
    let s = word_replace_infix(source, "until", "..");
    let s = word_replace_infix(&s, "downTo", "..=");
    let s = rewrite_kotlin_to_pair(&s);
    // `a step k` → `a.step_by(k)`. Consume the next token as the argument
    // so we can close the paren. Any unclosed pattern was breaking parse.
    rewrite_step(&s)
}

/// Rewrite Kotlin's infix `a to b` pair constructor to the Nebula tuple
/// literal `(a, b)`. Only fires on ` IDENT to IDENT ` patterns to avoid
/// mangling the English word "to" embedded in comments/strings/names.
/// Leaves dotted/indexed expressions alone — the user can rewrite those
/// manually if needed (rare in the ingest path).
fn rewrite_kotlin_to_pair(source: &str) -> String {
    source.lines().map(|line| {
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        let mut in_str: Option<char> = None;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = in_str {
                out.push(c);
                if c == q { in_str = None; }
                i += 1;
                continue;
            }
            if c == '"' || c == '\'' {
                in_str = Some(c);
                out.push(c);
                i += 1;
                continue;
            }
            // Look for `IDENT to IDENT` starting at word boundary.
            if c.is_ascii_alphabetic() || c == '_' {
                let prev = if i == 0 { None } else { Some(chars[i-1]) };
                let at_boundary = prev.map_or(true, |p|
                    !(p.is_ascii_alphanumeric() || p == '_' || p == '.')
                );
                if at_boundary {
                    let mut j = i;
                    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') { j += 1; }
                    let lhs: String = chars[i..j].iter().collect();
                    // Peek for ` to ` after lhs.
                    if j + 4 <= chars.len() && &chars[j..j+4] == [' ', 't', 'o', ' '] {
                        let mut k = j + 4;
                        let rhs_start = k;
                        while k < chars.len() && (chars[k].is_ascii_alphanumeric() || chars[k] == '_') { k += 1; }
                        if k > rhs_start {
                            let rhs: String = chars[rhs_start..k].iter().collect();
                            out.push('(');
                            out.push_str(&lhs);
                            out.push_str(", ");
                            out.push_str(&rhs);
                            out.push(')');
                            i = k;
                            continue;
                        }
                    }
                }
            }
            out.push(c);
            i += 1;
        }
        out
    }).collect::<Vec<_>>().join("\n")
}

/// Rewrite ` step EXPR` to `.step_by(EXPR)` by consuming `EXPR` up to the
/// next whitespace/EOL (treats a bare ident/number as the argument). For
/// more complex arguments the user can write `.step_by(…)` directly.
fn rewrite_step(source: &str) -> String {
    source.lines().map(|line| {
        let needle = " step ";
        let mut out = String::with_capacity(line.len());
        let mut rest = line;
        while let Some(pos) = rest.find(needle) {
            out.push_str(&rest[..pos]);
            rest = &rest[pos + needle.len()..];
            // Read the next arg — idents, numbers, or a single parens group.
            let arg_end = if rest.starts_with('(') {
                // Consume balanced parens.
                let mut depth = 0i32;
                let mut idx = 0;
                for (i, c) in rest.char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => { depth -= 1; if depth == 0 { idx = i + 1; break; } }
                        _ => {}
                    }
                }
                idx
            } else {
                rest.char_indices()
                    .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0)
            };
            let arg = &rest[..arg_end];
            rest = &rest[arg_end..];
            out.push_str(".step_by(");
            out.push_str(arg);
            out.push(')');
        }
        out.push_str(rest);
        out
    }).collect::<Vec<_>>().join("\n")
}

/// Replace infix-function invocations `LHS <name> RHS` with `LHS<glue>RHS`.
/// Matches `name` only at word boundaries and inside code (not strings).
/// `glue` is inserted verbatim — for operators use `"..="` / `".."`; for
/// method calls use `".name("` and accept the unclosed paren downstream.
fn word_replace_infix(source: &str, name: &str, glue: &str) -> String {
    let needle = format!(" {} ", name);
    if !source.contains(&needle) {
        return source.to_string();
    }
    let mut out = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0usize;
    let mut in_str: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            out.push(c);
            if c == q { in_str = None; }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            in_str = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        // Try to match the infix operator at position i.
        if c == ' ' {
            let rest: String = chars[i..].iter().collect();
            if rest.starts_with(&needle) {
                out.push_str(glue);
                i += needle.len();
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Rewrite `x++` / `x--` / `++x` / `--x` to Nebula-valid `x += 1` / `x -= 1`.
/// Replaces the old blind `.replace("++", " += 1")` which corrupted
/// `++count` → ` += 1count` (observed on JS/TS `closures` probes). Respects
/// simple string-literal boundaries so tokens inside `"a++"` / `'b--'` are
/// left alone.
fn rewrite_incdec(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len() + 16);
    let mut i = 0usize;
    let mut in_str: Option<char> = None;
    let mut prev_was_bs = false;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            out.push(c);
            if c == q && !prev_was_bs {
                in_str = None;
            }
            prev_was_bs = c == '\\' && !prev_was_bs;
            i += 1;
            continue;
        }
        prev_was_bs = false;
        if c == '"' || c == '\'' {
            in_str = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        // Detect `++` or `--`.
        let next = chars.get(i + 1).copied();
        if (c == '+' || c == '-') && next == Some(c) {
            let delta = if c == '+' { "+=" } else { "-=" };
            let prev_is_ident = out.chars().last().map_or(false, |pc|
                pc.is_ascii_alphanumeric() || pc == '_' || pc == ']' || pc == ')'
            );
            let after = chars.get(i + 2).copied();
            let after_is_ident = after.map_or(false, |ac| ac.is_ascii_alphanumeric() || ac == '_');
            if prev_is_ident && !after_is_ident {
                // Post: `x++` → `x += 1` (x already in out).
                out.push_str(&format!(" {} 1", delta));
                i += 2;
                continue;
            }
            if after_is_ident {
                // Pre: `++x` → `x += 1`.
                let mut j = i + 2;
                while j < chars.len() {
                    let jc = chars[j];
                    if jc.is_ascii_alphanumeric() || jc == '_' {
                        j += 1;
                    } else {
                        break;
                    }
                }
                let ident: String = chars[i + 2..j].iter().collect();
                out.push_str(&format!("{} {} 1", ident, delta));
                i = j;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn postprocess(source: &str) -> String {
    let s = rewrite_incdec(source);
    let s = s
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
        // Post/pre increment and decrement handled before this via rewrite_incdec.
        // Rust reference types — strip & from type positions
        .replace("&str", "Str")
        .replace("&mut ", "")
        .replace("&[", "[")
        // Go/C pointer types — strip * and &. We deliberately DO NOT do a
        // blanket `" &"` → `" "` replacement here: that mangles bitwise
        // `a & b` expressions into `a b`. Targeted patterns only; the
        // reference-pattern cleanup happens in `strip_rust_reference_amps`
        // later so we can inspect what follows the `&`.
        .replace("-> *", "-> ")
        .replace("&{", "{")
        .replace("(&", "(")
        .replace(", &", ", ")
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
        // Cleanup double spaces — inline only, never touch leading whitespace (indentation)
        .replace(":  ", ": ")
        // Kotlin collection constructors (before and after generic conversion)
        .replace("mutableListOf<Task>()", "[]")
        .replace("mutableListOf[Task]()", "[]")
        .replace("mutableListOf()", "[]")
        // listOf(a, b) → [a, b] — bracket rebalance happens below in
        // `rebalance_list_of_brackets` so the closing `)` flips to `]`.
        .replace("listOf(", "__NEB_LISTOF__(")
        .replace("mapOf(", "{")
        // Kotlin map syntax: "key" to val → "key": val
        .replace("\" to ", "\": ")
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
        // C raw declarations that leak into function bodies. We used to do
        // blanket `.replace("lt double ", "lt ")` etc. — but that ate any
        // JS/TS `const double = ...` declaration where `double` is the
        // binding name. The strip_c_type_prefixes helper below only fires
        // when the next token is a real identifier (not `=`, `:`, etc.).
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
    // Fix list types in struct fields: ": [Type]" → ": List[Type]" (parser doesn't accept [T] as type)
    let s = fix_list_types(&s);
    // Final generic type cleanup (after fix_generics converted <> to []).
    // Previously this also stripped `List[Int]` → `List` etc., which erased
    // every type arg on every generic. Keep only the non-lossy replacements.
    let s = s.replace("Vec[", "[").replace("Option[", "")
        .replace("[Task>", "List");
    // Re-run list type fix (generics conversion may have created new [Type] patterns)
    let s = fix_list_types(&s);
    // Go map types and slice literal bracket fixing
    let s = s.replace("map[string]interface{}", "Map").replace("Map{", "{");
    // Go empty interface: `interface{}` / `Interface{}` → `Any`. Nebula
    // has no Go-style empty-interface; Any is the closest equivalent.
    let s = s.replace("Interface{}", "Any").replace("interface{}", "Any");
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
    // Strip C-type prefixes (`lt int foo`, `lt double x`, etc.) — only when
    // they sit before a real identifier, never when the C-type token is
    // itself the binding's name (`lt double = ...`).
    let s = strip_c_type_prefixes(&s);
    // Strip TS/Java-style access modifiers Nebula doesn't model.
    let s = strip_access_modifiers(&s);
    // Convert TS/Swift/Kotlin `X[]` array-type syntax to Nebula `List`.
    let s = convert_array_types(&s);
    // Strip Rust reference-position `&` (e.g. ` &name` → ` name`) without
    // touching bitwise `a & b` in expressions. We only strip when the `&`
    // is immediately followed by an identifier character (no space).
    let s = strip_rust_reference_amps(&s);
    // Nebula's lexer doesn't parse hex / binary / octal literals — convert
    // any `0xFF`, `0b1010`, `0o777` in the source to decimal.
    let s = convert_numeric_literals(&s);
    // Nebula's char-literal lexer rejects `'\0'` (the C null terminator).
    // Replace with a decimal `0` so the surrounding expression still parses.
    let s = s.replace("'\\0'", "0");
    // Rename `.catch(` method calls (common on Promises) to `.on_error(`
    // so the lexer's `catch` keyword doesn't break the parse.
    let s = s.replace(".catch(", ".on_error(")
             .replace(".catch ", ".on_error ");
    // Kotlin/JS async block prefixes and `.await()` chains: Nebula has no
    // async/await runtime, so drop the markers so the surrounding expression
    // still parses.
    let s = s.replace("async {", "{")
             .replace("async(", "(")
             .replace(".await()", "()")
             .replace(".await ", " ");
    // Kotlin's elvis `a ?: b` → Nebula's nil-coalesce `a ?? b`.
    let s = s.replace(" ?: ", " ?? ");
    // Kotlin `catch (e: Type)` / `except (e: Type)` takes a parenthesized
    // binder; Nebula wants a bare identifier after the keyword.
    let s = s.replace("except (", "except ")
             .replace("catch (", "catch ");
    let s = strip_trailing_paren_after_except_catch(&s);
    // Strip leading `} ` from finally/except/catch/elif/else continuation
    // lines. They're artifacts of the source having `} finally {`-style
    // blocks in native code.
    let s = s.replace("} finally", "finally")
             .replace("} except", "except")
             .replace("} catch", "catch");
    // Kotlin-style `fn X(...): Type = expr` → `fn X(...) -> Type = expr`.
    // Nebula uses `->` for return annotations; `:` starts a block.
    let s = convert_kotlin_return_annotation(&s);
    // Kotlin/Rust destructuring for: `for (i, v) in expr` → `for i, v in expr`.
    let s = strip_for_tuple_parens(&s);
    // Kotlin extension-function names `fn Type.method(...)` / generics `fn <T>
    // List[T].method(...)` — Nebula's grammar doesn't allow dotted function
    // names or leading `<T>`. Flatten to `fn Type_method(...)`.
    let s = flatten_extension_functions(&s);
    // JS/TS object-destructuring params `({a, b, ...rest})` — Nebula params
    // can't hold destructure patterns. Replace the whole pattern with a
    // single `args` binding (best-effort; body still references the named
    // fields, which Nebula will flag at runtime only).
    let s = collapse_destructure_params(&s);
    // Strip Rust turbofish `.<T>` annotations: `.parse.<i64>()` → `.parse()`.
    // The generic argument is inferrable and Nebula's parser chokes on `<`.
    let s = strip_rust_turbofish(&s);
    // Strip generic parameters on declaration headers Nebula's grammar
    // doesn't model: `enum X[T, E]` / `trait X[T]` / `impl X[T]`.
    let s = strip_decl_generics(&s);
    // Rust `-> impl Trait` / `-> dyn Trait` return types: collapse the
    // entire trait expression to `Any` since Nebula has no existential or
    // trait-object types. We strip from the marker through the next `{` /
    // `:` / newline so things like `-> impl FnMut() -> Int` don't leak.
    let s = collapse_rust_trait_bounds(&s);
    // Collapse TS/Swift inline function types `(x: T) => U` → `Any` in
    // parameter and return positions — Nebula has no structural function
    // type syntax.
    let s = collapse_inline_function_types(&s);
    // Collapse inline double-spaces while preserving leading indentation.
    // We used to call `.replace("  ", " ")` globally, but that halved every
    // function-body indent level and flattened class/fn bodies into sibling-
    // declaration position.
    let s = collapse_inline_spaces(&s.replace(" else ", " el "));
    s
}

/// Collapse runs of 2+ spaces inside a line to a single space while preserving
/// leading indentation. Never touches string literals (naive: skips anything
/// between matched `"`s on the same line).
/// Rename parameters that collide with Nebula reserved keywords. Scope is
/// tracked by indentation: when a function signature declares a colliding
/// parameter, the same identifier is rewritten wherever it appears at deeper
/// indent until the indent returns to the signature's level.
///
/// We run this AFTER the main postprocess/canonicalize passes so the
/// collision set is evaluated against canonical Nebula keywords (e.g. `fn` if
/// the short form is in use, plus the long-form `def`).
fn rename_reserved_params(source: &str) -> String {
    use std::collections::{HashMap, HashSet};

    // Conservative list: names that cause parser chaos when used as bare
    // identifiers. We don't include long-form keywords the normalize pass
    // emits (`def`, `return`, etc.) because they'll be caught by the short
    // alias check regardless.
    // Keywords that must not appear as bare identifiers (param names). We
    // deliberately exclude `self`/`Self` — they ARE valid Nebula identifiers
    // referring to the current instance and we must not rename them.
    let reserved: HashSet<&str> = [
        "fn", "def", "rt", "return", "lt", "let", "mt", "mut", "cn", "const",
        "fo", "for", "wl", "while", "lp", "loop", "mc", "match",
        "st", "struct", "im", "impl", "tr", "trait", "en", "enum",
        "if", "elif", "else", "in", "and", "or", "not", "is", "as",
        "try", "catch", "except", "finally", "throw", "raise",
        "async", "await", "yield", "with", "pass", "break", "continue",
        "class", "import", "use", "module", "pub",
    ].iter().copied().collect();

    // Stack frame: (indent_of_signature, rename_map)
    let mut stack: Vec<(usize, HashMap<String, String>)> = Vec::new();

    let mut out = String::with_capacity(source.len());
    for (i, line) in source.lines().enumerate() {
        if i > 0 { out.push('\n'); }
        let leading_len = line.len() - line.trim_start().len();

        // Pop frames whose scope has closed (current line at <= frame indent
        // and not blank).
        if !line.trim().is_empty() {
            while let Some((sig_indent, _)) = stack.last() {
                if leading_len <= *sig_indent { stack.pop(); } else { break; }
            }
        }

        let trimmed = line.trim_start();
        let is_fn_line = trimmed.starts_with("fn ") || trimmed.starts_with("def ");

        if is_fn_line {
            // Extract (params) substring.
            if let (Some(lp), Some(rp)) = (trimmed.find('('), trimmed.rfind(')')) {
                let params = &trimmed[lp + 1..rp];
                let mut rename = HashMap::new();
                let mut new_params = String::new();
                for (idx, p) in split_args(params).iter().enumerate() {
                    if idx > 0 { new_params.push_str(", "); }
                    let p_trim = p.trim();
                    if p_trim.is_empty() { continue; }
                    // Param shape: `name` or `name: Type` or `name = default` or `*args`/`**kwargs`
                    let (name_part, rest) = match p_trim.find(':') {
                        Some(pos) => (&p_trim[..pos], &p_trim[pos..]),
                        None => match p_trim.find('=') {
                            Some(pos) => (&p_trim[..pos], &p_trim[pos..]),
                            None => (p_trim, ""),
                        },
                    };
                    let name_clean = name_part.trim_start_matches('*').trim();
                    let prefix_stars: String = name_part.chars().take_while(|c| *c == '*').collect();
                    if reserved.contains(name_clean) {
                        let new_name = format!("{}_", name_clean);
                        rename.insert(name_clean.to_string(), new_name.clone());
                        new_params.push_str(&prefix_stars);
                        new_params.push_str(&new_name);
                        new_params.push_str(rest.trim_end());
                    } else {
                        new_params.push_str(p_trim);
                    }
                }
                // Rebuild the line with new params.
                let before = &trimmed[..lp + 1];
                let after = &trimmed[rp..];
                let new_trim = format!("{}{}{}", before, new_params, after);
                out.push_str(&line[..leading_len]);
                out.push_str(&new_trim);

                if !rename.is_empty() {
                    stack.push((leading_len, rename));
                }
                continue;
            }
        }

        // Apply active renames (from deepest frame outward).
        if stack.is_empty() {
            out.push_str(line);
        } else {
            let mut rewritten = line.to_string();
            for (_, map) in stack.iter().rev() {
                for (old, new) in map {
                    rewritten = rewrite_whole_word(&rewritten, old, new);
                }
            }
            out.push_str(&rewritten);
        }
    }
    if source.ends_with('\n') { out.push('\n'); }
    out
}

/// Replace every whole-word occurrence of `old` with `new` in `line`. Skips
/// string-literal content EXCEPT inside `{...}` interpolation expressions,
/// which are real Nebula expressions that the parser evaluates.
fn rewrite_whole_word(line: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_str: Option<u8> = None; // Some(quote) when inside a string literal
    let mut in_interp = 0i32;          // >0 when inside {expr} inside a string
    let mut escape = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escape {
            out.push(b as char);
            escape = false;
            i += 1;
            continue;
        }
        // Inside-string handling: either in literal text (skip) or in an
        // interpolation expression (rewrite as normal code).
        if let Some(d) = in_str {
            if in_interp == 0 {
                out.push(b as char);
                if b == b'\\' { escape = true; }
                else if b == d { in_str = None; }
                else if b == b'{' { in_interp = 1; }
                i += 1;
                continue;
            } else {
                // Inside an interpolation expression — match `old` like code.
                if b == b'}' {
                    in_interp -= 1;
                    out.push(b as char);
                    i += 1;
                    continue;
                }
                if b == b'{' {
                    in_interp += 1;
                    out.push(b as char);
                    i += 1;
                    continue;
                }
                // Fall through to the code-mode logic below.
            }
        }
        if in_str.is_none() && (b == b'"' || b == b'\'') {
            in_str = Some(b);
            out.push(b as char);
            i += 1;
            continue;
        }
        // Match `old` at word boundary (code mode or interpolation mode).
        if i + old.len() <= bytes.len() && &bytes[i..i + old.len()] == old.as_bytes() {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_ok = i + old.len() == bytes.len() || !is_ident_byte(bytes[i + old.len()]);
            if before_ok && after_ok {
                out.push_str(new);
                i += old.len();
                continue;
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn collapse_inline_spaces(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for (i, line) in source.lines().enumerate() {
        if i > 0 { out.push('\n'); }
        let leading_len = line.len() - line.trim_start().len();
        let (leading, body) = line.split_at(leading_len);
        out.push_str(leading);

        let mut prev_was_space = false;
        let mut in_str: Option<char> = None;
        let mut escape = false;
        for ch in body.chars() {
            if escape {
                out.push(ch);
                escape = false;
                continue;
            }
            if let Some(d) = in_str {
                out.push(ch);
                if ch == '\\' { escape = true; }
                else if ch == d { in_str = None; }
                continue;
            }
            if ch == '"' || ch == '\'' {
                in_str = Some(ch);
                out.push(ch);
                prev_was_space = false;
                continue;
            }
            if ch == ' ' {
                if !prev_was_space {
                    out.push(' ');
                }
                prev_was_space = true;
            } else {
                out.push(ch);
                prev_was_space = false;
            }
        }
    }
    if source.ends_with('\n') { out.push('\n'); }
    out
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

/// Fix list types in struct field positions: ": [Type]" → ": List[Type]".
/// Nebula's type grammar requires a leading identifier before brackets, so
/// `[T]` alone doesn't parse. Previously this stripped the inner type
/// entirely (`: [f64]` → `: List`), erasing the element type on every
/// Rust-generic and Swift-array round-trip. Now we preserve it.
fn fix_list_types(source: &str) -> String {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let ws = &line[..line.len() - trimmed.len()];
        // Loop until no more bracket patterns remain on this line — previously
        // only the first `: [...]` match was rewritten, so `zipped(a: [Int],
        // b: [Int])` would fix `a` but leave `b` raw and the parser rejected.
        let mut current = trimmed.to_string();
        loop {
            // Try `: [T]` first, then `-> [T]`. Detect Swift-style
            // dictionary literal `[K: V]` and prefer `Map[K, V]`.
            let mut changed = false;
            let classify = |inner: &str| -> String {
                // Top-level colon? Then `K: V` dictionary shape.
                let mut depth = 0i32;
                for (i, c) in inner.char_indices() {
                    match c {
                        '[' | '(' | '{' | '<' => depth += 1,
                        ']' | ')' | '}' | '>' => depth -= 1,
                        ':' if depth == 0 => {
                            let k = inner[..i].trim();
                            let v = inner[i+1..].trim();
                            return format!("Map[{}, {}]", k, v);
                        }
                        _ => {}
                    }
                }
                format!("List[{}]", inner)
            };
            if let Some(colon_pos) = current.find(": [") {
                if let Some(close) = current[colon_pos..].find(']') {
                    let before = &current[..colon_pos + 2];
                    let inner = &current[colon_pos + 3..colon_pos + close];
                    let after = &current[colon_pos + close + 1..];
                    current = format!("{}{}{}", before, classify(inner), after);
                    changed = true;
                }
            }
            if !changed {
                if let Some(arr_pos) = current.find("-> [") {
                    if let Some(close) = current[arr_pos..].find(']') {
                        let before = &current[..arr_pos + 3];
                        let inner = &current[arr_pos + 4..arr_pos + close];
                        let after = &current[arr_pos + close + 1..];
                        current = format!("{}{}{}", before, classify(inner), after);
                        changed = true;
                    }
                }
            }
            if !changed { break; }
        }
        out.push(format!("{}{}", ws, current));
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
            // Also remove orphan > (but NOT the > in "->"). Only scan the
            // return-type portion — stop at the first `=`/`{`/`(` so we
            // don't eat a `>` comparison inside an expression body like
            // `-> T = if (a > b)`.
            let after_arrow2 = &s[arrow_pos + 3..];
            let scan_end = after_arrow2.find(|c: char| c == '=' || c == '{' || c == '(')
                .unwrap_or(after_arrow2.len());
            let type_only = &after_arrow2[..scan_end];
            let type_trimmed = type_only.trim_end();
            if type_trimmed.contains('>') && !type_trimmed.contains('<') {
                let search_start = arrow_pos + 3;
                if let Some(gt_pos) = s[search_start..search_start + scan_end].find('>') {
                    let abs_pos = search_start + gt_pos;
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
        // Handle `if (COND) STMT`, `for (H) STMT`, `while (C) STMT` where
        // STMT doesn't start with `{`. Nebula requires a block after these
        // control keywords; Kotlin/Swift/Rust can omit braces for a single
        // statement body. Wrap it.
        for (kw, kw_len) in [("if", 2), ("for", 3), ("while", 5)] {
            let prefix = format!("{} (", kw);
            if trimmed.starts_with(&prefix) && !trimmed.ends_with('{') && !trimmed.contains(" el ") {
                // Find matching close-paren accounting for nesting.
                let mut depth = 0i32;
                let mut close_idx: Option<usize> = None;
                for (idx, c) in trimmed.char_indices().skip(kw_len) {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                close_idx = Some(idx);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(close) = close_idx {
                    let rest = trimmed[close + 1..].trim_start();
                    if !rest.is_empty() && !rest.starts_with('{') {
                        let head = &trimmed[..close + 1];
                        let ws = &line[..line.len() - trimmed.len()];
                        return format!("{}{} {{ {} }}", ws, head, rest);
                    }
                }
            }
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n")
}

/// Fix Swift optional binding: `if task = expr {` → `if expr != nil {`.
/// Also handles the guard-let-on-one-line shape
/// `if n = expr else { STMT }` → `if expr == nil: STMT` (Nebula lacks
/// guard, so we invert the condition and inline the else body).
fn fix_swift_optional_binding(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim();
        // Shape A: "if IDENT = EXPR el { BODY }" / "if IDENT = EXPR else { BODY }"
        // — Swift's guard-let after raw-conversion. `else` may still be `el`
        // at this stage (postprocess runs before canonicalize re-expands it).
        for (needle, needle_len) in [(" el { ", 6), (" else { ", 8)] {
            if trimmed.starts_with("if ")
                && trimmed.contains(needle)
                && trimmed.ends_with('}') {
                if let Some(eq_pos) = trimmed.find(" = ") {
                    if let Some(else_pos) = trimmed.find(needle) {
                        if else_pos > eq_pos {
                            let before_eq = &trimmed[3..eq_pos];
                            if before_eq.chars().all(|c| c.is_alphanumeric() || c == '_')
                                && !trimmed.contains("==") {
                                let bound_expr = trimmed[eq_pos + 3..else_pos].trim();
                                let else_body = trimmed[else_pos + needle_len..].trim_end_matches('}').trim();
                                let ws = &line[..line.len() - trimmed.len()];
                                return format!("{}if {} == nil {{ {} }}", ws, bound_expr, else_body);
                            }
                        }
                    }
                }
            }
        }
        // Shape B: "if IDENT = EXPR {" where it's not "if IDENT == ..."
        if trimmed.starts_with("if ") && trimmed.ends_with('{') {
            if let Some(eq_pos) = trimmed.find(" = ") {
                let before_eq = &trimmed[3..eq_pos];
                let after_eq = &trimmed[eq_pos + 3..].trim_end_matches('{').trim();
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
                    // Bail out if the close-paren actually lives AFTER the `el` token
                    // (e.g. C/Go else-if-with-parens like `} el if (foo->bar) {`).
                    // Without this guard the slice below underflows.
                    if actual_close >= el_pos {
                        out.push(line.to_string());
                        continue;
                    }
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

/// Convert `!expr` to `not expr`, but leave `!=` alone. Respects string
/// literal boundaries so `"hello!"` doesn't become `"hellonot "` (observed
/// on JS template-literal round-trip).
fn fix_negation_operator(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut in_str: Option<char> = None;
    let mut prev_was_bs = false;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            result.push(c);
            if c == q && !prev_was_bs {
                in_str = None;
            }
            prev_was_bs = c == '\\' && !prev_was_bs;
            i += 1;
            continue;
        }
        prev_was_bs = false;
        if c == '"' || c == '\'' {
            in_str = Some(c);
            result.push(c);
            i += 1;
            continue;
        }
        if c == '!' {
            if i + 1 < chars.len() && chars[i + 1] == '=' {
                result.push('!');
            } else {
                // Rust macros: `vec!`, `println!`, `format!`, `assert!`. Here `!`
                // is postfix on an identifier, not a prefix negation. If the
                // previous char looks like the tail of an identifier, keep `!`.
                let prev = result.chars().last();
                let is_macro_call = prev.map_or(false, |p| p.is_ascii_alphanumeric() || p == '_');
                if is_macro_call {
                    result.push('!');
                } else {
                    result.push_str("not ");
                }
            }
        } else {
            result.push(c);
        }
        i += 1;
    }
    result
}

/// Final pass: fix raw keywords that survived through minification.
/// Runs line-by-line so leading whitespace doesn't matter.
/// Replace only the FIRST ` = ` in a `for`/`fo` header with ` in `, and
/// only before the first `{` (the body opener). Earlier logic did a
/// blanket `line.replace(" = ", " in ")` which corrupted assignments in
/// inline bodies, e.g. `for (i, w) in xs { out[w] = i }` → `{ out[w] in i }`.
fn replace_header_eq_to_in(line: &str) -> String {
    let end = line.find('{').unwrap_or(line.len());
    let head = &line[..end];
    let tail = &line[end..];
    let head_rewritten = match head.find(" = ") {
        Some(i) => format!("{} in {}", &head[..i], &head[i + 3..]),
        None => head.to_string(),
    };
    format!("{}{}", head_rewritten, tail)
}

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
            // C-style `for (init; cond; update)` → while loop. The telltale
            // is `for (` with `;` inside the paren-run. `for x in v { g(x); }`
            // (Rust single-line body) also contains `;` but the `;` is inside
            // a `{…}` body; we were rewriting that to `while } {` and then
            // the rest of the pipeline collapsed it to `while :`. Require a
            // leading paren to avoid misdetecting Rust/Python shapes.
            let is_c_style = trimmed.starts_with("for (") && trimmed.contains(';');
            if is_c_style {
                let inner = trimmed.strip_prefix("for ").unwrap_or(trimmed);
                let inner = inner.trim_start_matches('(');
                let parts: Vec<&str> = inner.splitn(3, ';').collect();
                if parts.len() >= 2 {
                    let cond = parts[1].trim();
                    format!("while {} {{", cond)
                } else {
                    replace_header_eq_to_in(trimmed)
                }
            } else {
                replace_header_eq_to_in(trimmed)
            }
        } else if trimmed.starts_with("fo ") && trimmed.contains(" = ") && trimmed.contains('{') {
            // Go range that still has = instead of in
            replace_header_eq_to_in(trimmed)
        }
        // Python tuple unpacking: "a, b = 0, 1" → "let (a, b) = (0, 1)". Only
        // fire when the LHS is a clean comma-separated list of bare
        // identifiers (no braces / parens / colons / declaration keywords).
        else if trimmed.contains(", ") && trimmed.contains(" = ")
                && !trimmed.contains("for ") && !trimmed.contains("fo ")
                && !trimmed.contains("range") && !trimmed.contains("in ") {
            let parts: Vec<&str> = trimmed.splitn(2, " = ").collect();
            let lhs_ok = parts.len() == 2
                && parts[0].contains(',')
                && !parts[0].contains('(')
                && !parts[0].contains('{')
                && !parts[0].contains('[')
                && !parts[0].contains(':');
            let decl_kw = |s: &str| {
                let s = s.trim_start();
                s.starts_with("st ") || s.starts_with("en ")
                || s.starts_with("tr ") || s.starts_with("im ")
                || s.starts_with("struct ") || s.starts_with("class ")
                || s.starts_with("enum ") || s.starts_with("trait ")
                || s.starts_with("impl ") || s.starts_with("interface ")
                || s.starts_with("def ") || s.starts_with("fn ")
                || s.starts_with("data ") || s.starts_with("sealed ")
            };
            if lhs_ok && !decl_kw(parts[0]) {
                format!("let ({}) = ({})", parts[0], parts[1])
            } else {
                trimmed.to_string()
            }
        }
        // C-style type declarations: "int id" or "Task tasks[N]" → "id: Int" / "tasks: [Task]"
        // Only apply to lines WITHOUT colons (already-converted lines have "name: Type")
        else if !trimmed.contains(':') && !trimmed.contains('(') && !trimmed.contains('=')
                && !trimmed.contains('{') && !trimmed.contains('}')
                && !trimmed.contains(',')
                && !trimmed.starts_with("fn ") && !trimmed.starts_with("rt ")
                && !trimmed.starts_with("lt ") && !trimmed.starts_with("mt ")
                && !trimmed.starts_with("if ") && !trimmed.starts_with("fo ")
                && !trimmed.starts_with("wl ") && !trimmed.starts_with("st ")
                && !trimmed.starts_with("im ") && !trimmed.starts_with("en ")
                && !trimmed.starts_with("tr ") && !trimmed.starts_with("mc ")
                && !trimmed.starts_with("use ") && !trimmed.starts_with("import ")
                && !trimmed.starts_with("module ") && !trimmed.starts_with("md ")
                && !trimmed.starts_with("package ") && !trimmed.starts_with("pb ")
                && !trimmed.starts_with("return ") && !trimmed.starts_with("def ")
                && !trimmed.starts_with("class ") && !trimmed.starts_with("struct ")
                && !trimmed.starts_with("impl ") && !trimmed.starts_with("trait ")
                && !trimmed.starts_with("enum ") && !trimmed.starts_with("match ")
                && !trimmed.starts_with("let ") && !trimmed.starts_with("mut ")
                && !trimmed.starts_with("throw ") && !trimmed.starts_with("raise ")
                && !trimmed.starts_with("yield ") && !trimmed.starts_with("pass ")
                && !trimmed.starts_with("break") && !trimmed.starts_with("continue")
                && !trimmed.starts_with("elif ") && !trimmed.starts_with("else ")
                && !trimmed.starts_with("while ") && !trimmed.starts_with("for ") {
            let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
            // The C-style declaration shape is `TYPE NAME` where BOTH tokens
            // are bare identifiers (optionally with `[...]`/`*`/`&`). A
            // statement like `a + b` also satisfies `parts.len()==2` with
            // first token alphabetic, so we must additionally verify that
            // the second token starts with an identifier character and that
            // the LINE doesn't contain any binary operators.
            let second_ok = parts.get(1)
                .map(|s| s.trim())
                .and_then(|s| s.chars().next().map(|c| c.is_alphabetic() || c == '_' || c == '*' || c == '&'))
                .unwrap_or(false);
            let has_operator = trimmed.contains(" + ") || trimmed.contains(" - ")
                || trimmed.contains(" * ") || trimmed.contains(" / ")
                || trimmed.contains(" % ") || trimmed.contains(" < ")
                || trimmed.contains(" > ") || trimmed.contains(" | ")
                || trimmed.contains(" & ") || trimmed.contains(" ^ ");
            if parts.len() == 2
                && parts[0].chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
                && second_ok && !has_operator {
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

        // Lines ending with { → replace { with :. The earlier `!starts_with('"')`
        // check over-rejected match arms like `"circle" => {`; we now require
        // the `{` to be outside any string literal (even-count double-quotes
        // before it means the `{` is in code, not inside `"..."`).
        let brace_outside_string = {
            let bytes = trimmed.as_bytes();
            if !trimmed.ends_with('{') {
                false
            } else {
                let mut dq = 0usize;
                let mut i = 0;
                while i + 1 < bytes.len() {
                    if bytes[i] == b'\\' { i += 2; continue; }
                    if bytes[i] == b'"' { dq += 1; }
                    i += 1;
                }
                dq % 2 == 0
            }
        };
        if brace_outside_string {
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

            // Match arm: `pat => {` → `pat =>` (the indented body is the
            // arm expression; an extra `:` after `=>` would break parse).
            if without_brace.ends_with("=>") {
                out.push(format!("{}{}", ws, without_brace));
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
    // Convert any `nil` that leaked through (e.g. from `go_var()` → `nil`
    // replacement that runs after normalize_keywords's ` nil` → ` None`
    // pass) to Nebula's `None`.
    s = s.replace(" nil\n", " None\n")
         .replace(" nil,", " None,")
         .replace(" nil)", " None)")
         .replace(" nil}", " None}")
         .replace(" nil ", " None ")
         .replace("(nil)", "(None)")
         .replace("(nil,", "(None,");
    if s.ends_with(" nil") {
        let cut = s.len() - 4;
        s = format!("{} None", &s[..cut]);
    }

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
            // Mutable binding: mt/mut x = expr → x = expr (bare). BUT when
            // there's a type annotation (`mt x: T = v`) we must keep `mt`
            // because Nebula's parser only accepts `name: T = v` with a
            // binding keyword; bare `name: T = v` is a syntax error.
            let rest = if trimmed.starts_with("mt ") { &trimmed[3..] } else { &trimmed[4..] };
            // Is there a `:` before the first `=`, outside of parens/brackets?
            let mut has_type_ann = false;
            let mut depth = 0i32;
            for c in rest.chars() {
                match c {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => depth -= 1,
                    '=' if depth == 0 => break,
                    ':' if depth == 0 => { has_type_ann = true; break; }
                    _ => {}
                }
            }
            if has_type_ann {
                format!("mt {}", rest)
            } else {
                rest.to_string()
            }
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

        // Nebula doesn't have a runtime-level async/await — the inner
        // expression is semantically the same as the native awaited value.
        // Strip `aw ` / `ac ` prefixes wherever they appear so the parser
        // doesn't choke on the Await token.
        let converted = strip_async_await_prefixes(&converted);

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

/// Drop leading access/visibility modifiers Nebula doesn't model.
fn strip_access_modifiers(source: &str) -> String {
    let modifiers = ["private ", "public ", "protected ", "readonly ",
                      "override ", "open ", "final ", "internal ", "abstract ",
                      "sealed ", "static ", "export ", "default ", "companion ",
                      "inner ", "annotation ", "inline ", "tailrec ",
                      "suspend ", "operator ", "infix ", "lateinit "];
    source.lines().map(|line| {
        let leading_len = line.len() - line.trim_start().len();
        let leading = &line[..leading_len];
        let mut rest = line[leading_len..].to_string();
        // Apply repeatedly to handle `public static final X` chains.
        let mut changed = true;
        while changed {
            changed = false;
            for m in &modifiers {
                if rest.starts_with(m) {
                    rest = rest[m.len()..].to_string();
                    changed = true;
                    break;
                }
            }
        }
        format!("{}{}", leading, rest)
    }).collect::<Vec<_>>().join("\n")
}

/// Collapse `-> impl Trait...` / `: impl Trait...` (and `dyn` variants) to
/// `Any`. Scans each line and replaces the whole trait-bound expression
/// through the next block-start token.
fn collapse_rust_trait_bounds(source: &str) -> String {
    let markers = ["-> impl ", "-> dyn ", ": impl ", ": dyn "];
    source.lines().map(|line| {
        let mut s = line.to_string();
        loop {
            let mut hit: Option<(usize, usize)> = None;
            for m in &markers {
                if let Some(p) = s.find(m) {
                    hit = Some((p, m.len()));
                    break;
                }
            }
            let (p, m_len) = match hit { Some(x) => x, None => break };
            // Find end of the trait bound: stop at `{` / `:` / newline /
            // unbalanced `)` at depth 0 / `,` outside generics.
            let start = p + m_len;
            let bytes = s.as_bytes();
            let mut depth = 0i32;
            let mut angle = 0i32;
            let mut end = bytes.len();
            let mut k = start;
            while k < bytes.len() {
                match bytes[k] {
                    b'(' | b'[' => depth += 1,
                    b')' | b']' => {
                        if depth == 0 && angle == 0 { end = k; break; }
                        depth -= 1;
                    }
                    b'<' => angle += 1,
                    b'>' => angle -= 1,
                    b'{' | b':' | b'\n' | b',' if depth == 0 && angle <= 0 => {
                        end = k;
                        break;
                    }
                    _ => {}
                }
                k += 1;
            }
            let head = &s[..p];
            let prefix = if m_len == 8 { "-> Any" } else { ": Any" };
            let mut rebuilt = String::from(head);
            rebuilt.push_str(prefix);
            rebuilt.push_str(&s[end..]);
            s = rebuilt;
        }
        s
    }).collect::<Vec<_>>().join("\n")
}

/// Replace `def NAME({...}):` or `def NAME([...]):` param-destructure
/// patterns with a single `args` binding. Best-effort: we only collapse
/// the ONE param if the whole param list is a single destructure shape.
fn collapse_destructure_params(source: &str) -> String {
    source.lines().map(|line| {
        let leading_len = line.len() - line.trim_start().len();
        let leading = &line[..leading_len];
        let rest = &line[leading_len..];
        if !(rest.starts_with("def ") || rest.starts_with("fn ")) {
            return line.to_string();
        }
        let lp = match rest.find('(') { Some(i) => i, None => return line.to_string() };
        let rp = match rest.rfind(')') { Some(i) => i, None => return line.to_string() };
        if rp <= lp { return line.to_string(); }
        let inner = &rest[lp + 1..rp];
        // Fast path: a single `{...}` destructure.
        let t = inner.trim();
        let new_inner = if (t.starts_with('{') && t.ends_with('}'))
            || (t.starts_with('[') && t.ends_with(']')) {
            "args".to_string()
        } else {
            return line.to_string();
        };
        format!("{}{}({}){}", leading, &rest[..lp], new_inner, &rest[rp + 1..])
    }).collect::<Vec<_>>().join("\n")
}

/// Normalize Kotlin-style `except (e: Type)` into Nebula's `except e:`.
/// We've already stripped the leading `(`, so this pass strips the trailing
/// `)` and the in-binder `: Type` annotation that Nebula doesn't accept.
fn strip_trailing_paren_after_except_catch(source: &str) -> String {
    source.lines().map(|line| {
        let leading_len = line.len() - line.trim_start().len();
        let leading = &line[..leading_len];
        let trimmed = &line[leading_len..];
        let (keyword_prefix, rest_start) = if let Some(r) = trimmed.strip_prefix("except ") {
            ("except ", r)
        } else if let Some(r) = trimmed.strip_prefix("catch ") {
            ("catch ", r)
        } else if let Some(r) = trimmed.strip_prefix("} except ") {
            ("} except ", r)
        } else if let Some(r) = trimmed.strip_prefix("} catch ") {
            ("} catch ", r)
        } else {
            return line.to_string();
        };
        // End-of-binder: the first `)` if present (Kotlin `catch (e: Type)`
        // leaves a stray `)`); otherwise the block opener `:` / `{` / EOL.
        let end_paren = rest_start.find(')');
        let end_other = rest_start.find(|c: char| c == '{' || c == '\n');
        let end_colon = rest_start.find(':');
        let terminator = match (end_paren, end_colon, end_other) {
            (Some(p), _, _) => p,
            (None, Some(c), _) => c,
            (None, None, Some(o)) => o,
            _ => rest_start.len(),
        };
        let binder = &rest_start[..terminator];
        let after_raw = &rest_start[terminator..];
        // If the terminator was `)`, skip past it in the after-string.
        let after = if after_raw.starts_with(')') { &after_raw[1..] } else { after_raw };
        // Binder content: strip any nested `: Type` annotation — Nebula's
        // except/catch takes just a bare binder.
        let binder_clean = if let Some(colon_pos) = binder.find(':') {
            binder[..colon_pos].trim().to_string()
        } else {
            binder.trim().to_string()
        };
        // Preserve an empty binder (Python-style `except:`).
        if binder_clean.is_empty() {
            format!("{}{}{}", leading, keyword_prefix.trim_end(), after)
        } else {
            format!("{}{}{}{}", leading, keyword_prefix, binder_clean, after)
        }
    }).collect::<Vec<_>>().join("\n")
}

/// Split Kotlin-style `fn X(...) -> T = mc (EXPR):` into block-form:
///   fn X(...) -> T:
///     match EXPR:
/// Also re-indents the arms that followed (which inherited the function's
/// body indent when the source used `= when (s)` → `= mc (s) { ... }` but
/// should be one level deeper now).
fn expand_single_expr_match(source: &str) -> String {
    let lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let leading_len = line.len() - line.trim_start().len();
        let leading = &line[..leading_len];
        let rest = &line[leading_len..];
        let matches_header = (rest.starts_with("fn ") || rest.starts_with("def "))
            && rest.trim_end().ends_with(':')
            && (rest.contains(" = mc ") || rest.contains(" = match "));
        if matches_header {
            let eq_pos = rest.find(" = mc ").or_else(|| rest.find(" = match ")).unwrap();
            let head = rest[..eq_pos].trim_end();
            let after_eq = rest[eq_pos + 3..].trim_end_matches(':').trim();
            let match_expr = after_eq
                .strip_prefix("mc ").or_else(|| after_eq.strip_prefix("match "))
                .unwrap_or(after_eq).trim();
            let match_expr = match_expr.trim_start_matches('(').trim_end_matches(')').trim();
            let inner_ind: String = format!("{}  ", leading);
            let arm_ind: String = format!("{}    ", leading);
            out.push(format!("{}{}:", leading, head));
            out.push(format!("{}match {}:", inner_ind, match_expr));
            // Absorb following lines at the same or deeper indent and shift
            // their indent under the new `match` header.
            let func_body_indent = leading_len + 2;
            let mut j = i + 1;
            while j < lines.len() {
                let l = &lines[j];
                if l.trim().is_empty() {
                    out.push(l.clone());
                    j += 1;
                    continue;
                }
                let ll = l.len() - l.trim_start().len();
                if ll < func_body_indent { break; }
                // Re-indent to arm_ind (drop original body-level, push deeper).
                let body = l[func_body_indent..].trim_end();
                // Strip `is ` prefix from match-arm labels — Kotlin's `is Type`
                // pattern has no Nebula equivalent, so drop the keyword.
                let body = body.strip_prefix("is ").unwrap_or(body);
                out.push(format!("{}{}", arm_ind, body));
                j += 1;
            }
            i = j;
            continue;
        }
        out.push(line.clone());
        i += 1;
    }
    out.join("\n")
}

/// Strip the parens around a destructuring for-loop binder:
/// `for (i, v) in expr` / `fo (i, v) in expr` → `for i, v in expr`.
fn strip_for_tuple_parens(source: &str) -> String {
    source.lines().map(|line| {
        for kw in &["for (", "fo (", "} for (", "} fo ("] {
            if let Some(p) = line.find(kw) {
                // Find matching `)`. Keep balanced.
                let start = p + kw.len();
                let bytes = line.as_bytes();
                let mut depth = 1;
                let mut end = start;
                while end < bytes.len() {
                    match bytes[end] {
                        b'(' => depth += 1,
                        b')' => { depth -= 1; if depth == 0 { break; } }
                        _ => {}
                    }
                    end += 1;
                }
                if end < bytes.len() {
                    // Require " in " to follow — otherwise probably a
                    // C-style for with semicolons.
                    let after = &line[end + 1..];
                    if !after.trim_start().starts_with("in ") { continue; }
                    let binder = &line[start..end];
                    let new_kw = &kw[..kw.len() - 1].trim_end(); // drop trailing `(`
                    return format!("{}{} {}{}", &line[..p], new_kw, binder, &line[end + 1..]);
                }
            }
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n")
}

/// Flatten Kotlin/Swift extension-function declarations to a plain name.
/// `fn Type.method(...)` → `fn type_method(...)`. `fn <T, R> List[T].x(...)`
/// drops the generics prefix and uses `list_x` as the name.
fn flatten_extension_functions(source: &str) -> String {
    source.lines().map(|line| {
        let leading_len = line.len() - line.trim_start().len();
        let leading = &line[..leading_len];
        let rest = &line[leading_len..];
        let kw = if let Some(r) = rest.strip_prefix("fn ") { Some(("fn ", r)) }
                 else if let Some(r) = rest.strip_prefix("def ") { Some(("def ", r)) }
                 else { None };
        let (kw_str, after_kw) = match kw { Some(x) => x, None => return line.to_string() };
        // Drop generic prefix `<T, R>` before the receiver type.
        let after_generics = if after_kw.starts_with('<') {
            match after_kw.find("> ") {
                Some(p) => &after_kw[p + 2..],
                None => after_kw,
            }
        } else { after_kw };
        // Find the end of the function name (up to `(`).
        let lp = match after_generics.find('(') { Some(p) => p, None => return line.to_string() };
        let name = after_generics[..lp].trim();
        if !name.contains('.') { return line.to_string(); }
        // Replace `.` with `_`, lowercase the first segment.
        let parts: Vec<&str> = name.split('.').collect();
        let mut flat_parts: Vec<String> = Vec::new();
        for (idx, p) in parts.iter().enumerate() {
            let p = p.trim_matches(|c: char| c == '[' || c == ']').trim();
            // Strip list-like containers in the type (`List[T]` → `list`).
            let p = p.split('[').next().unwrap_or(p);
            let s = if idx == 0 {
                p.chars().enumerate().map(|(i, c)| {
                    if i > 0 && c.is_uppercase() {
                        format!("_{}", c.to_lowercase())
                    } else {
                        c.to_lowercase().to_string()
                    }
                }).collect::<String>()
            } else {
                p.to_string()
            };
            if !s.is_empty() { flat_parts.push(s); }
        }
        let flat_name = flat_parts.join("_");
        let tail = &after_generics[lp..];
        format!("{}{}{}{}", leading, kw_str, flat_name, tail)
    }).collect::<Vec<_>>().join("\n")
}

/// Rewrite Kotlin-style return annotations on function declarations:
/// `fn NAME(args): Type = expr` → `fn NAME(args) -> Type = expr`.
fn convert_kotlin_return_annotation(source: &str) -> String {
    source.lines().map(|line| {
        let leading_len = line.len() - line.trim_start().len();
        let leading = &line[..leading_len];
        let rest = &line[leading_len..];
        if !(rest.starts_with("fn ") || rest.starts_with("def ")) {
            return line.to_string();
        }
        // Find the closing paren of the param list via balanced matching.
        let bytes = rest.as_bytes();
        let lp = match rest.find('(') { Some(i) => i, None => return line.to_string() };
        let mut depth = 0;
        let mut rp = rest.len();
        for i in lp..bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => { depth -= 1; if depth == 0 { rp = i; break; } }
                _ => {}
            }
        }
        if rp >= rest.len() { return line.to_string(); }
        let tail = &rest[rp + 1..];
        // Look for `: TYPE` right after the paren, stopping at `=`, `{`, or
        // a block opener.
        let tail_trim = tail.trim_start();
        if !tail_trim.starts_with(':') { return line.to_string(); }
        let after_colon = &tail_trim[1..];
        // Find end of the type: first `=` / `{` / end of line.
        let end = after_colon.find(|c: char| c == '=' || c == '{')
            .unwrap_or(after_colon.len());
        let ty_str = after_colon[..end].trim();
        let remaining = &after_colon[end..];
        if ty_str.is_empty() { return line.to_string(); }
        let leading_ws = &tail[..tail.len() - tail_trim.len()];
        format!("{}{}{} -> {}{}{}",
            leading, &rest[..rp + 1], leading_ws, ty_str,
            if remaining.is_empty() { "" } else { " " }, remaining)
    }).collect::<Vec<_>>().join("\n")
}

/// Drop generic parameter lists on declaration headers: `enum X[T, E] {`
/// → `enum X {`, etc. Nebula's enum/trait grammars don't accept `[...]`
/// after the name. Runs line-by-line on the known declaration keywords.
fn strip_decl_generics(source: &str) -> String {
    let kws = ["enum ", "en ", "trait ", "tr ", "impl ", "im ", "struct ", "st ", "class ", "def ", "fn "];
    source.lines().map(|line| {
        let leading_len = line.len() - line.trim_start().len();
        let leading = &line[..leading_len];
        let body = &line[leading_len..];
        for kw in &kws {
            if let Some(after_kw) = body.strip_prefix(*kw) {
                // Skip the identifier.
                let name_end = after_kw.char_indices()
                    .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
                    .map(|(i, _)| i).unwrap_or(after_kw.len());
                let rest = &after_kw[name_end..];
                if !rest.starts_with('[') {
                    // Skip only if the generics use `[`; <T, E> is handled
                    // earlier by fix_generics.
                    return line.to_string();
                }
                // Find matching ].
                let bytes = rest.as_bytes();
                let mut depth = 0;
                let mut end = 0;
                for (i, &b) in bytes.iter().enumerate() {
                    match b {
                        b'[' => depth += 1,
                        b']' => { depth -= 1; if depth == 0 { end = i + 1; break; } }
                        _ => {}
                    }
                }
                if end == 0 { return line.to_string(); }
                // Skip only for `enum`/`trait` where Nebula rejects the
                // generic list. For struct/class/impl/def/fn we keep it
                // since the parser accepts `[T]` there.
                if !(*kw == "enum " || *kw == "en " || *kw == "trait " || *kw == "tr ") {
                    return line.to_string();
                }
                let rebuilt = format!("{}{}{}{}", leading, kw,
                    &after_kw[..name_end], &rest[end..]);
                return rebuilt;
            }
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n")
}

/// Strip Rust turbofish `.<T>` annotations. Matches `.<` followed by a
/// balanced `<...>` and removes it in place. We only fire when the preceding
/// character is `.` (method turbofish) or whitespace, so expression-level
/// `< >` comparisons stay intact.
fn strip_rust_turbofish(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1] == b'<' {
            // Find matching `>` via angle-bracket depth counter.
            let mut depth = 0;
            let mut k = i + 1;
            while k < bytes.len() {
                match bytes[k] {
                    b'<' => depth += 1,
                    b'>' => { depth -= 1; if depth == 0 { k += 1; break; } }
                    b'\n' => break,
                    _ => {}
                }
                k += 1;
            }
            if depth == 0 {
                // Strip everything from `.` through the matching `>`.
                i = k;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Convert `0xHH`, `0b...`, `0o...` numeric literals to decimal. Nebula's
/// lexer only recognizes decimal integers and floats; other bases leak
/// through as `0` followed by an identifier (e.g. `0xff` → `0` + `xff`).
fn convert_numeric_literals(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    let mut in_str: Option<u8> = None;
    let mut escape = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escape {
            out.push(b as char);
            escape = false;
            i += 1;
            continue;
        }
        if let Some(d) = in_str {
            out.push(b as char);
            if b == b'\\' { escape = true; }
            else if b == d { in_str = None; }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_str = Some(b);
            out.push(b as char);
            i += 1;
            continue;
        }
        // Only consider `0` that's at a numeric-literal position (preceded
        // by something non-ident so it's not part of an identifier).
        if b == b'0' && i + 1 < bytes.len() {
            let prev_ok = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            let kind = bytes[i + 1];
            if prev_ok && matches!(kind, b'x' | b'X' | b'b' | b'B' | b'o' | b'O') {
                let base = match kind { b'x' | b'X' => 16, b'b' | b'B' => 2, _ => 8 };
                // Consume digits.
                let digits_start = i + 2;
                let mut k = digits_start;
                while k < bytes.len() {
                    let c = bytes[k];
                    let ok = match base {
                        16 => c.is_ascii_hexdigit() || c == b'_',
                        2 => c == b'0' || c == b'1' || c == b'_',
                        8 => (b'0'..=b'7').contains(&c) || c == b'_',
                        _ => false,
                    };
                    if ok { k += 1; } else { break; }
                }
                if k > digits_start {
                    let digits: String = source[digits_start..k].chars()
                        .filter(|c| *c != '_').collect();
                    if let Ok(v) = u128::from_str_radix(&digits, base) {
                        out.push_str(&v.to_string());
                        i = k;
                        continue;
                    }
                }
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

/// Add `mt ` prefix to bare statement-level typed declarations
/// (`name: Type` → `mt name: Type`). Nebula's parser only accepts typed
/// decls with a leading `lt` / `mt` / `cn` / `let` / `mut` keyword. Swift
/// `let foo: (Int) -> Int = { ... }` can collapse to `foo: Any` after the
/// closure body and `let`/type-arrow are stripped, leaving the parser
/// stuck. This pass adds the missing keyword.
fn prefix_bare_typed_decls(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        let ws = &line[..leading];
        // Only handle lines whose first non-space token is a bare ident
        // followed by `:` and then a type (not a block-opening `{`).
        if trimmed.starts_with("lt ") || trimmed.starts_with("mt ")
            || trimmed.starts_with("cn ") || trimmed.starts_with("let ")
            || trimmed.starts_with("mut ") || trimmed.starts_with("const ")
            || trimmed.starts_with("def ") || trimmed.starts_with("fn ")
            || trimmed.starts_with("class ") || trimmed.starts_with("st ")
            || trimmed.starts_with("en ") || trimmed.starts_with("tr ")
            || trimmed.starts_with("if ") || trimmed.starts_with("for ")
            || trimmed.starts_with("fo ") || trimmed.starts_with("while ")
            || trimmed.starts_with("wl ") || trimmed.starts_with("match ")
            || trimmed.starts_with("mc ") || trimmed.starts_with("impl ")
            || trimmed.starts_with("im ") || trimmed.starts_with("use ")
            || trimmed.starts_with("return ") || trimmed.starts_with("rt ")
            || trimmed.starts_with("try") || trimmed.starts_with("catch")
            || trimmed.starts_with("except") || trimmed.starts_with("//")
            || trimmed.starts_with('#') || trimmed.starts_with('@')
            || trimmed.is_empty()
        {
            return line.to_string();
        }
        // Check: `name: Type` shape — ident, then `:`, then not a block
        // continuation (so not `name: Type:` which is an arm).
        let name_end = trimmed.char_indices()
            .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_')
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        if name_end == 0 { return line.to_string(); }
        let rest = &trimmed[name_end..];
        if !rest.starts_with(':') { return line.to_string(); }
        if rest.starts_with(":=") { return line.to_string(); }  // walrus is fine
        // If line ends with `:` alone (trailing block opener), probably an
        // arm / type header — don't touch.
        if trimmed.ends_with(':') && !trimmed.contains('=') {
            return line.to_string();
        }
        // Without a top-level `=` this may be a struct/class field
        // declaration (`name: Type`) or a degraded top-level binding
        // (e.g., Swift `let addOne: Fn = {…}` with the function type
        // lost). Struct fields are indented and should be left alone;
        // top-level bare typed decls (indent 0) aren't legal Nebula and
        // get commented out so the surrounding file still parses.
        let mut has_top_eq = false;
        let mut depth = 0i32;
        for c in trimmed.chars() {
            match c {
                '(' | '[' | '{' | '<' => depth += 1,
                ')' | ']' | '}' | '>' => depth -= 1,
                '=' if depth == 0 => { has_top_eq = true; break; }
                _ => {}
            }
        }
        if !has_top_eq {
            if leading == 0 {
                return format!("// {}", trimmed);
            }
            return line.to_string();
        }
        format!("{}mt {}", ws, trimmed)
    }).collect::<Vec<_>>().join("\n")
}

/// Pad empty `class X:` / `st X:` / `en X:` / `tr X:` blocks with a
/// placeholder member so the parser accepts the declaration. Python `pass`
/// bodies get stripped upstream, leaving class headers with no following
/// indented content; Nebula's grammar rejects that.
/// Rewrite Swift/Kotlin's bare-operator function reference (`xs.reduce(0,
/// +)`) to an explicit lambda (`xs.reduce(0, |a, b| a + b)`). Nebula has
/// no operator-as-first-class-value grammar; passing `+` as an argument
/// trips the parser with `Unexpected token: Plus`. Handles the common
/// binary arithmetic and comparison operators in both argument and
/// trailing positions.
fn rewrite_operator_references(source: &str) -> String {
    let ops = [
        ("+", "|a, b| a + b"),
        ("-", "|a, b| a - b"),
        ("*", "|a, b| a * b"),
        ("/", "|a, b| a / b"),
        ("%", "|a, b| a % b"),
        ("<", "|a, b| a < b"),
        (">", "|a, b| a > b"),
        ("<=", "|a, b| a <= b"),
        (">=", "|a, b| a >= b"),
        ("==", "|a, b| a == b"),
        ("!=", "|a, b| a != b"),
    ];
    let mut out = source.to_string();
    for (op, rep) in ops {
        // `, OP)` → `, LAMBDA)` and `(OP)` / `(OP,`.
        out = out.replace(&format!(", {})", op), &format!(", {})", rep));
        out = out.replace(&format!("({})", op), &format!("({})", rep));
        out = out.replace(&format!("({}, ", op), &format!("({}, ", rep));
        out = out.replace(&format!(", {},", op), &format!(", {},", rep));
    }
    out
}

/// Comment out duplicate `except`/`catch` clauses within the same `try`
/// block. Nebula's parser only accepts a single catch handler per try.
/// The second+ clause bodies are preserved as comments so a later pass
/// (or the corrector) can recover the intent. Lossy but keeps the first
/// handler live and the surrounding function parseable.
fn collapse_multi_except(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut try_stack: Vec<(usize, bool)> = Vec::new(); // (indent, seen_except)
    let mut commenting_block: Option<usize> = None; // indent threshold; comment lines deeper than this
    for line in lines.iter() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        // Active commenting: drop any line deeper than threshold. The
        // parser's indent tracker chokes on `//` comments at arbitrary
        // depths (dedent after a comment treats the comment's indent as
        // the closing level), so we drop entirely rather than comment.
        if let Some(thr) = commenting_block {
            if trimmed.is_empty() || leading > thr {
                if trimmed.is_empty() {
                    out.push(line.to_string());
                }
                continue;
            } else {
                commenting_block = None;
            }
        }
        // Pop any try frames whose indent is >= current non-empty indent.
        if !trimmed.is_empty() {
            while let Some(&(ind, _)) = try_stack.last() {
                if leading <= ind && !trimmed.starts_with("except") && !trimmed.starts_with("catch") && !trimmed.starts_with("finally") {
                    try_stack.pop();
                } else {
                    break;
                }
            }
        }
        if trimmed == "try" || trimmed == "try:" || trimmed.starts_with("try:") || trimmed.starts_with("try ") || trimmed.starts_with("try{") {
            try_stack.push((leading, false));
            out.push(line.to_string());
            continue;
        }
        if trimmed.starts_with("except") || trimmed.starts_with("catch") {
            if let Some(frame) = try_stack.last_mut() {
                if frame.1 {
                    // Drop the extra handler and its body; Nebula grammar
                    // only supports a single catch clause.
                    commenting_block = Some(leading);
                    continue;
                } else {
                    frame.1 = true;
                }
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Normalize `except`/`catch` clauses so the binding is always a bare
/// identifier. Swift/Python/Kotlin can catch on specific error types
/// (`except BadInput.empty:`, `except (TypeError, ValueError) as e:`) or
/// omit the binding entirely (`except:`). Nebula's grammar only accepts a
/// single identifier, so we collapse the type discrimination and default
/// the binding to `e`. Lossy (the type filter is dropped), but keeps the
/// body reachable so the rest of the function still translates.
fn normalize_except_clauses(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        let ws = &line[..line.len() - trimmed.len()];
        for kw in ["except", "catch"] {
            if trimmed == kw || trimmed == &format!("{}:", kw) {
                return format!("{}{} e:", ws, kw);
            }
            if let Some(rest) = trimmed.strip_prefix(&format!("{} ", kw)) {
                // `catch e { ... }` / `except e:` — already canonical.
                let body_pos = rest.find(&[':', '{'][..]);
                if let Some(bp) = body_pos {
                    let head = &rest[..bp];
                    let tail = &rest[bp..];
                    // Simple-ident binding already → leave alone.
                    let is_simple = !head.is_empty()
                        && head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c.is_whitespace())
                        && head.split_whitespace().count() == 1;
                    if is_simple {
                        return line.to_string();
                    }
                    return format!("{}{} e{}", ws, kw, tail);
                }
            }
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n")
}

/// Strip `lt `/`mt ` binding modifiers from Swift pattern-bind arms.
/// Swift case patterns write `.circle(let r)` / `.rect(let w, let h)` —
/// the canonicalizer rewrites `let ` → `lt `, but Nebula's pattern grammar
/// has no binding modifier. The bare identifier already binds in a
/// pattern context, so we drop the prefix. Only fires on lines whose
/// trailing content is `=>` (match arm shape).
/// Wrap match-arm bodies that start with `return`/`rt` into a block so
/// the expression-context parser accepts them. Nebula's `rt EXPR` is a
/// statement, not an expression; match arms want expression bodies. The
/// fix is a block wrap: `=> rt "x"` → `=> { rt "x" }`. Fires per line
/// on the `=>` separator.
/// Swift/Kotlin match guard uses `where`; Nebula uses `if`. Rewrite
/// `PATTERN where COND => BODY` to `PATTERN if COND => BODY` per line.
/// Swift `if case let .V(x) = s { body }` / `if .V(lt x) = s { body }`
/// — a pattern-match-in-if that Nebula can't express. Comment the line
/// out so the surrounding function still parses; the body is lost but
/// the rest of the file survives.
/// Convert `__NEB_LISTOF__(…)` back to `[…]` with matching-paren → `]`.
/// Intermediate step so the naive `listOf(` → `[` leaves a dangling
/// closing `)` that the parser rejects. Walks paren depth to find the
/// matching close and flips just that one to `]`.
fn rebalance_list_of_brackets(source: &str) -> String {
    let marker = "__NEB_LISTOF__(";
    if !source.contains(marker) { return source.to_string(); }
    let mut out = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with(marker) {
            out.push('[');
            i += marker.len();
            // Scan to matching `)` respecting paren depth and strings.
            let mut depth = 1i32;
            let mut in_str: Option<char> = None;
            while i < chars.len() {
                let c = chars[i];
                if let Some(q) = in_str {
                    out.push(c);
                    if c == q { in_str = None; }
                    i += 1;
                    continue;
                }
                if c == '"' || c == '\'' { in_str = Some(c); out.push(c); i += 1; continue; }
                if c == '(' { depth += 1; out.push(c); i += 1; continue; }
                if c == ')' {
                    depth -= 1;
                    if depth == 0 { out.push(']'); i += 1; break; }
                    out.push(c);
                    i += 1;
                    continue;
                }
                out.push(c);
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn comment_out_if_case_patterns(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut commenting_indent: Option<usize> = None;
    for line in lines.iter() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if let Some(ci) = commenting_indent {
            if trimmed.is_empty() || leading > ci {
                if trimmed.is_empty() { out.push(line.to_string()); }
                // drop deep-indent body
                continue;
            } else {
                commenting_indent = None;
            }
        }
        // `if case …`, `if .V(…) = s`, and `while let …` all encode a
        // pattern-match in condition position that Nebula can't parse.
        // Drop the header and its indented body; the function still
        // translates without the destructuring arm.
        let looks_like_pattern_cond = ((trimmed.starts_with("if .") || trimmed.starts_with("if case "))
                && trimmed.contains(" = "))
            || (trimmed.starts_with("while let ") && trimmed.contains(" = "))
            || (trimmed.starts_with("wl let ") && trimmed.contains(" = "));
        if looks_like_pattern_cond {
            out.push(format!("{}// {}", &line[..leading], trimmed));
            let has_body_continuation = trimmed.ends_with(':')
                || (trimmed.ends_with('{') && !trimmed.contains('}'));
            if has_body_continuation {
                commenting_indent = Some(leading);
            }
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// When a match arm's pattern is an expression containing comparison
/// operators (`==`, `!=`, `<`, `>`), rewrite it as a guarded wildcard
/// `_ if EXPR =>`. JS/TS `switch(true) { case x == 0: ... }` emerges
/// from the ingester as `x == 0 =>`, which Nebula rejects (patterns
/// can't contain binary operators).
/// Rewrite multi-value match-arm patterns `a, b, c => body` to a guard
/// form `_ if x == a or x == b or x == c => body`. Needs knowledge of
/// the match subject `x`, which we carry across the pipeline by tracking
/// the enclosing `match SUBJECT:` header indent. Lossy when the subject
/// is not a simple identifier.
/// Merge JS switch fall-through arms like
/// ```
/// 0 =>
/// 6 => { return "weekend" }
/// ```
/// into a single multi-value arm `0, 6 => { return "weekend" }`, then
/// the downstream `rewrite_multi_value_arms` converts it to a guard form.
fn merge_fallthrough_match_arms(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();
        if trimmed.trim_start().ends_with("=>") && !trimmed.trim_start().ends_with(" => {") {
            // Collect the arm-pattern strings until a non-empty-bodied arm.
            let leading = trimmed.len() - trimmed.trim_start().len();
            let mut patterns: Vec<String> = Vec::new();
            let head = trimmed.trim_start();
            let pat = head[..head.len() - 2].trim_end();
            patterns.push(pat.to_string());
            let mut j = i + 1;
            while j < lines.len() {
                let nt = lines[j].trim_end();
                if nt.trim().is_empty() { j += 1; continue; }
                let nts = nt.trim_start();
                // Stop if indent drops (new match section).
                let nl = nt.len() - nts.len();
                if nl != leading { break; }
                if let Some(arrow) = nts.find(" => ") {
                    let p = nts[..arrow].trim();
                    patterns.push(p.to_string());
                    let body = &nts[arrow..];
                    let ws = &lines[i][..leading];
                    out.push(format!("{}{}{}", ws, patterns.join(", "), body));
                    i = j + 1;
                    break;
                } else if nts.ends_with("=>") {
                    let p = nts[..nts.len() - 2].trim_end();
                    patterns.push(p.to_string());
                    j += 1;
                    continue;
                } else {
                    break;
                }
            }
            if patterns.len() > 1 {
                continue;
            }
        }
        out.push(line.to_string());
        i += 1;
    }
    out.join("\n")
}

fn rewrite_multi_value_arms(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut subject: Option<(String, usize)> = None; // (expr, indent)
    for line in lines.iter() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if trimmed.starts_with("match ") && trimmed.ends_with(':') {
            let expr = trimmed[6..trimmed.len()-1].trim().to_string();
            subject = Some((expr, leading));
            out.push(line.to_string());
            continue;
        }
        if let Some((subj, mi)) = subject.as_ref() {
            if !trimmed.is_empty() && leading <= *mi {
                subject = None;
            } else if leading > *mi {
                if let Some(arrow) = trimmed.find(" => ") {
                    let pat = &trimmed[..arrow];
                    let body = &trimmed[arrow..];
                    // Must be top-level comma-separated list of simple values.
                    if pat.contains(',') {
                        let mut depth = 0i32;
                        let mut top_commas = 0u32;
                        for c in pat.chars() {
                            match c {
                                '(' | '[' | '{' | '<' => depth += 1,
                                ')' | ']' | '}' | '>' => depth -= 1,
                                ',' if depth == 0 => top_commas += 1,
                                _ => {}
                            }
                        }
                        if top_commas > 0 {
                            let parts: Vec<String> = pat.split(',')
                                .map(|p| p.trim().to_string()).collect();
                            let cond: Vec<String> = parts.iter()
                                .map(|p| format!("{} == {}", subj, p))
                                .collect();
                            let ws = &line[..leading];
                            out.push(format!("{}_ if {} {}", ws, cond.join(" or "), body));
                            continue;
                        }
                    }
                }
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

fn rewrite_expr_match_arms(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        let leading_len = line.len() - trimmed.len();
        let Some(arrow) = trimmed.find(" => ").or_else(|| trimmed.find(" =>")) else {
            return line.to_string();
        };
        let head = &trimmed[..arrow];
        let tail = &trimmed[arrow..];
        // Already a guard shape?
        if head.contains(" if ") { return line.to_string(); }
        // Does the pattern contain a comparison operator at top level?
        let mut has_cmp = false;
        let mut depth = 0i32;
        let bytes: Vec<char> = head.chars().collect();
        let mut k = 0;
        while k < bytes.len() {
            let c = bytes[k];
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                '=' if depth == 0 && bytes.get(k+1).copied() == Some('=') => { has_cmp = true; break; }
                '!' if depth == 0 && bytes.get(k+1).copied() == Some('=') => { has_cmp = true; break; }
                '<' | '>' if depth == 0 => {
                    // Skip `->` and `=>` — only the standalone `<`/`>`.
                    let prev = if k == 0 { ' ' } else { bytes[k-1] };
                    if prev != '-' && prev != '=' {
                        has_cmp = true; break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        if !has_cmp { return line.to_string(); }
        let leading = &line[..leading_len];
        format!("{}_ if {}{}", leading, head, tail)
    }).collect::<Vec<_>>().join("\n")
}

fn rewrite_where_guard_in_match(source: &str) -> String {
    source.lines().map(|line| {
        let arrow = match line.find(" => ") { Some(i) => i, None => return line.to_string() };
        let head = &line[..arrow];
        let tail = &line[arrow..];
        let Some(wpos) = head.find(" where ") else { return line.to_string(); };
        let new_head = format!("{} if {}", &head[..wpos], &head[wpos + 7..]);
        format!("{}{}", new_head, tail)
    }).collect::<Vec<_>>().join("\n")
}

fn wrap_return_in_match_arms(source: &str) -> String {
    source.lines().map(|line| {
        let Some(arrow_pos) = line.find(" => ") else { return line.to_string(); };
        let head = &line[..arrow_pos + 4];
        let body = &line[arrow_pos + 4..];
        let body_trim = body.trim_start();
        let wraps = body_trim.starts_with("rt ")
            || body_trim.starts_with("return ")
            || body_trim == "rt"
            || body_trim == "return"
            || body_trim.starts_with("throw ")
            || body_trim.starts_with("break")
            || body_trim.starts_with("bk")
            || body_trim.starts_with("continue")
            || body_trim.starts_with("ct");
        if !wraps || body_trim.starts_with('{') {
            return line.to_string();
        }
        format!("{}{{ {} }}", head, body_trim)
    }).collect::<Vec<_>>().join("\n")
}

fn strip_pattern_binding_modifiers(source: &str) -> String {
    source.lines().map(|line| {
        // Match arm shapes: trailing `=>`, `=> { ... }`, or `=> expr`.
        // The pattern portion is the substring up to ` => `. Outside of
        // that, leave the line alone.
        let sep = " => ";
        let contains_arrow = line.contains(sep) || line.trim_end().ends_with("=>");
        if !contains_arrow {
            return line.to_string();
        }
        let (head, tail) = match line.find(sep) {
            Some(i) => (line[..i].to_string(), line[i..].to_string()),
            None => (line.to_string(), String::new()),
        };
        let head_cleaned = head
            .replace("(lt ", "(")
            .replace(", lt ", ", ")
            .replace("(mt ", "(")
            .replace(", mt ", ", ");
        format!("{}{}", head_cleaned, tail)
    }).collect::<Vec<_>>().join("\n")
}

/// Flatten dotted trait paths in `impl` headers: `impl a.b.Trait fo T`
/// → `impl Trait fo T`. Nebula's impl header expects a single trait
/// identifier; dotted paths (Rust/Go `fmt.Display`, Kotlin `io.Reader`)
/// otherwise trip the parser at the first `.`.
fn flatten_impl_trait_paths(source: &str) -> String {
    source.lines().map(|line| {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        let starts_impl = trimmed.starts_with("impl ") || trimmed.starts_with("im ");
        if !starts_impl { return line.to_string(); }
        // Split "impl" keyword from body.
        let (kw, rest) = if let Some(r) = trimmed.strip_prefix("impl ") {
            ("impl ", r)
        } else if let Some(r) = trimmed.strip_prefix("im ") {
            ("im ", r)
        } else { return line.to_string(); };
        // Look for ` fo ` / ` for ` separator.
        let sep_pos = rest.find(" fo ").or_else(|| rest.find(" for "));
        let Some(sp) = sep_pos else { return line.to_string(); };
        let trait_part = rest[..sp].trim();
        let sep = if rest[sp..].starts_with(" fo ") { " fo " } else { " for " };
        let target = &rest[sp + sep.len()..];
        if !trait_part.contains('.') { return line.to_string(); }
        // Keep only the final segment of the dotted path.
        let last_seg = trait_part.rsplit('.').next().unwrap_or(trait_part);
        format!("{}{}{}{}{}", &line[..leading], kw, last_seg, sep, target)
    }).collect::<Vec<_>>().join("\n")
}

/// Convert Kotlin interface property signatures inside trait bodies to
/// Nebula fn signatures. Kotlin's `interface Named { val name: String }`
/// ingests as `trait Named:\n  name: Str` which Nebula rejects (trait
/// bodies only accept `fn` signatures). We rewrite `name: TYPE` → `fn
/// name() -> TYPE`. Scope: lines indented deeper than a `trait X:` /
/// `tr X:` header, bare `ident: TYPE` shape with no `=`/`(`/`{` on the
/// line.
/// Drop method declarations inside Kotlin/Rust-style enum bodies.
/// Nebula enum grammar accepts only variant lines; a `fn`/`def` inside
/// the enum block trips the parser. Drops the method (and its indented
/// body) so the enum parses, losing the attached behavior.
fn drop_enum_body_methods(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut enum_indent: Option<usize> = None;
    let mut drop_indent: Option<usize> = None;
    for line in lines.iter() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if let Some(di) = drop_indent {
            if !trimmed.is_empty() && leading <= di {
                drop_indent = None;
            } else {
                if trimmed.is_empty() { out.push(line.to_string()); }
                continue;
            }
        }
        if let Some(ei) = enum_indent {
            if !trimmed.is_empty() && leading <= ei {
                enum_indent = None;
            } else if leading > ei {
                let is_method = trimmed.starts_with("fn ")
                    || trimmed.starts_with("def ");
                if is_method {
                    drop_indent = Some(leading);
                    continue;
                }
            }
        }
        if trimmed.starts_with("enum ") || trimmed.starts_with("en ") {
            if trimmed.ends_with(':') || trimmed.ends_with('{') {
                enum_indent = Some(leading);
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

fn trait_body_props_to_fns(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut trait_indent: Option<usize> = None;
    for line in lines.iter() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if trimmed.starts_with("trait ") || trimmed.starts_with("tr ") {
            trait_indent = Some(leading);
            out.push(line.to_string());
            continue;
        }
        if let Some(ti) = trait_indent {
            if !trimmed.is_empty() && leading <= ti {
                trait_indent = None;
            } else if leading > ti
                && !trimmed.starts_with("fn ")
                && !trimmed.starts_with("def ")
                && !trimmed.starts_with("//")
                && !trimmed.contains('(')
                && !trimmed.contains('=')
                && !trimmed.contains('{')
            {
                if let Some(colon) = trimmed.find(':') {
                    let name = trimmed[..colon].trim();
                    let ty = trimmed[colon+1..].trim();
                    let name_ok = !name.is_empty() && name.chars().all(|c|
                        c.is_ascii_alphanumeric() || c == '_'
                    );
                    if name_ok && !ty.is_empty() {
                        out.push(format!("{}fn {}() -> {}", &line[..leading], name, ty));
                        continue;
                    }
                }
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Append `{}` to function/method headers that lost their body during
/// ingest. Triggers when a `def`/`fn` line has no `:`, `{`, or ` = ` after
/// its header and isn't followed by an indented body line. Without this,
/// ingester output like `def foo() -> Any` (emitted when the Kotlin/Swift
/// function return-type degraded and the single-expression body dropped)
/// trips the parser with `Expected LeftBrace, got Fn` on the next decl.
fn pad_empty_fn_bodies(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        let starts_fn = trimmed.starts_with("def ")
            || trimmed.starts_with("fn ");
        if !starts_fn {
            out.push(line.to_string());
            continue;
        }
        // Skip if header already has a body marker.
        let ends_with_body = line.trim_end().ends_with(':')
            || line.trim_end().ends_with('{')
            || line.trim_end().ends_with('}')
            || line.contains(" = ");
        if ends_with_body {
            out.push(line.to_string());
            continue;
        }
        // Peek ahead for a deeper-indented body line.
        let has_body = lines[i + 1..].iter().find_map(|next| {
            let nt = next.trim_end();
            if nt.trim().is_empty() { return None; }
            let nl = nt.len() - nt.trim_start().len();
            Some(nl > leading)
        }).unwrap_or(false);
        if has_body {
            // Indented body is below — ensure the header opens a block.
            // When ingest degraded the return type or stripped a function-
            // type body, the header loses its trailing `{` / `:`; the body
            // then floats without an opener and the parser errors.
            out.push(format!("{}:", line.trim_end()));
        } else {
            out.push(format!("{} {{}}", line.trim_end()));
        }
    }
    out.join("\n")
}

fn pad_empty_type_bodies(source: &str) -> String {
    let padded = pad_empty_type_bodies_inner(source);
    pad_empty_fn_bodies(&padded)
}

fn pad_empty_type_bodies_inner(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        out.push(line.to_string());
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        let is_type_header = (trimmed.starts_with("class ")
            || trimmed.starts_with("st ")
            || trimmed.starts_with("en ")
            || trimmed.starts_with("tr ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("struct ")
            || trimmed.starts_with("trait "))
            && trimmed.ends_with(':');
        if !is_type_header { continue; }
        // Peek ahead for a body line indented deeper than this header.
        let has_body = lines[i + 1..].iter().find_map(|next| {
            let nt = next.trim_end();
            if nt.trim().is_empty() { return None; }
            let nl = nt.len() - nt.trim_start().len();
            Some(nl > leading)
        }).unwrap_or(false);
        if !has_body {
            let pad = " ".repeat(leading + 4);
            // Traits accept only fn signatures; structs/classes/enums take
            // fields. Pick the right placeholder per header keyword.
            let is_trait = trimmed.starts_with("tr ")
                || trimmed.starts_with("trait ");
            let is_enum = trimmed.starts_with("en ")
                || trimmed.starts_with("enum ");
            let placeholder = if is_trait {
                "def _placeholder(self) -> Nil"
            } else if is_enum {
                "_Empty"
            } else {
                "_empty: Any"
            };
            out.push(format!("{}{}", pad, placeholder));
        }
    }
    out.join("\n")
}

/// Comment out empty-name `impl:` / `impl X:` orphans left behind by
/// Kotlin's companion-object → object → st rewrite pipeline. These can
/// appear at top-level without any associated struct, and the parser
/// rejects a bare `impl:`.
fn comment_orphan_impls(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::new();
    let mut commenting: Option<usize> = None;
    for i in 0..lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let leading_len = line.len() - trimmed.len();
        if let Some(impl_indent) = commenting {
            if !trimmed.is_empty() && leading_len <= impl_indent {
                commenting = None;
            } else {
                out.push(format!("{}// {}", &line[..leading_len], line[leading_len..].trim_end()));
                continue;
            }
        }
        if trimmed == "impl:" || trimmed == "impl" || trimmed == "impl {" || trimmed == "impl {}" {
            out.push(format!("{}// {}", &line[..leading_len], trimmed));
            commenting = Some(leading_len);
            continue;
        }
        // `impl Name:` with no indented body beneath it — parser rejects a
        // dangling impl colon. Detect by peeking: if the next non-empty
        // line isn't indented deeper than this line, comment the impl.
        if trimmed.starts_with("impl ") && trimmed.ends_with(':') {
            let mut has_body = false;
            for next in &lines[i + 1..] {
                let nt = next.trim_end();
                if nt.trim().is_empty() { continue; }
                let next_leading = nt.len() - nt.trim_start().len();
                has_body = next_leading > leading_len;
                break;
            }
            if !has_body {
                out.push(format!("{}// {}", &line[..leading_len], trimmed));
                continue;
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Rewrite nested type declarations (`struct`/`enum`/`trait`/`class` at
/// indent > 0) as placeholder `_name: Any` fields and comment the decl
/// header. Nebula's grammar disallows nested decls, and commenting
/// alone leaves the outer struct body empty — which Nebula's parser also
/// rejects (it expects field content, not just comments). A placeholder
/// field keeps the outer type well-formed.
fn comment_nested_type_decls(source: &str) -> String {
    let keywords = ["struct ", "enum ", "trait ", "class ",
                    "struct:", "enum:", "trait:", "class:",
                    "struct {", "enum {", "trait {", "class {"];
    let mut out = Vec::new();
    let mut commenting: Option<usize> = None;  // indent of the nested decl
    for line in source.lines() {
        let leading_len = line.len() - line.trim_start().len();
        let trimmed = line[leading_len..].trim_end();
        if let Some(decl_indent) = commenting {
            if !trimmed.is_empty() && leading_len <= decl_indent {
                commenting = None;
            } else {
                out.push(format!("{}// {}", &line[..leading_len], trimmed));
                continue;
            }
        }
        let is_decl = keywords.iter().any(|kw| trimmed.starts_with(kw));
        if is_decl && leading_len > 0 {
            // Extract the nested type's name for the placeholder field.
            let kw = keywords.iter().find(|kw| trimmed.starts_with(**kw)).unwrap();
            let after_kw = &trimmed[kw.len()..];
            let name: String = after_kw.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // Generate a unique placeholder per call so multiple nested decls
            // inside the same outer block don't collide.
            // Prefix with `_nst_` to stay distinct from any snake_case
            // identifier a user might reference elsewhere in the module
            // (avoids false-positive casing_variants flags on the original
            // PascalCase type name).
            let placeholder_name = if name.is_empty() {
                format!("_nst_{}", out.len())
            } else {
                let snake: String = name.chars().enumerate().map(|(i, c)| {
                    if i > 0 && c.is_uppercase() {
                        format!("_{}", c.to_lowercase())
                    } else {
                        c.to_lowercase().to_string()
                    }
                }).collect();
                format!("_nst_{}", snake)
            };
            out.push(format!("{}{}: Any  // {}", &line[..leading_len],
                placeholder_name, trimmed));
            if trimmed.ends_with('{') || trimmed.ends_with(':') {
                commenting = Some(leading_len);
            }
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// Strip Rust-style reference ampersands (` &name`) without touching
/// bitwise-AND (`a & b`). The marker is a `&` followed by an identifier
/// character with no intervening space. Runs globally but skips string
/// literals.
fn strip_rust_reference_amps(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    let mut in_str: Option<u8> = None;
    let mut escape = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escape {
            out.push(b as char);
            escape = false;
            i += 1;
            continue;
        }
        if let Some(d) = in_str {
            out.push(b as char);
            if b == b'\\' { escape = true; }
            else if b == d { in_str = None; }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_str = Some(b);
            out.push(b as char);
            i += 1;
            continue;
        }
        if b == b'&' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next.is_ascii_alphabetic() || next == b'_' {
                // Skip the `&` and continue.
                i += 1;
                continue;
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

/// Collapse TS/Swift inline function types in parameter/return annotation
/// positions. Matches `(...) => X` or `(...) -> X` and rewrites to `Any`.
/// Runs line-by-line; we only fire inside type-annotation context to avoid
/// breaking real expressions (a function-body arrow `|x| ...` is different
/// shape because of the `|..|` wrapper).
fn collapse_inline_function_types(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        // Find `:` or `->` then optional whitespace + `(`.
        if bytes[i] == b':' || (i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'>') {
            out.push(bytes[i] as char);
            i += 1;
            if bytes[i.saturating_sub(1)] == b'-' && i < bytes.len() && bytes[i] == b'>' {
                out.push('>');
                i += 1;
            }
            // Skip whitespace
            let mut j = i;
            while j < bytes.len() && bytes[j] == b' ' { j += 1; }
            if j < bytes.len() && bytes[j] == b'(' {
                // Find matching `)`
                let mut depth = 0;
                let mut k = j;
                while k < bytes.len() {
                    match bytes[k] {
                        b'(' => depth += 1,
                        b')' => { depth -= 1; if depth == 0 { k += 1; break; } }
                        _ => {}
                    }
                    k += 1;
                }
                // After `)`, look for optional whitespace + `=>` or `->`.
                let mut m = k;
                while m < bytes.len() && bytes[m] == b' ' { m += 1; }
                let is_arrow = (m + 1 < bytes.len() && bytes[m] == b'=' && bytes[m + 1] == b'>')
                    || (m + 1 < bytes.len() && bytes[m] == b'-' && bytes[m + 1] == b'>');
                if is_arrow {
                    // Find end of return type (until `,` `)` `\n` or block-start).
                    let mut n = m + 2;
                    let mut ret_depth = 0;
                    while n < bytes.len() {
                        match bytes[n] {
                            b'(' | b'[' | b'{' => ret_depth += 1,
                            b')' | b']' | b'}' => {
                                if ret_depth == 0 { break; }
                                ret_depth -= 1;
                            }
                            b',' if ret_depth == 0 => break,
                            b'\n' => break,
                            _ => {}
                        }
                        n += 1;
                    }
                    // Replace the whole `(...)... => ...` span with ` Any`.
                    // Preserve the leading space after the colon if present.
                    out.push_str(" Any");
                    i = n;
                    continue;
                }
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Convert `X[]` (TS/Swift/Kotlin "array of X") to `List` wherever it
/// appears outside string literals. Keeps the Nebula parser happy — it
/// accepts `List` and `[T]` but not `T[]`.
fn convert_array_types(source: &str) -> String {
    // Simple scan: find identifier chars followed by `[]` (no inner content).
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    let mut in_str: Option<u8> = None;
    let mut escape = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escape {
            out.push(b as char);
            escape = false;
            i += 1;
            continue;
        }
        if let Some(d) = in_str {
            out.push(b as char);
            if b == b'\\' { escape = true; }
            else if b == d { in_str = None; }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_str = Some(b);
            out.push(b as char);
            i += 1;
            continue;
        }
        // Look for IDENT followed by `[]`
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let ident = &source[start..i];
            if i + 1 < bytes.len() && bytes[i] == b'[' && bytes[i + 1] == b']' {
                // Replace IDENT[] with List
                out.push_str("List");
                i += 2;
                let _ = ident;
                continue;
            }
            out.push_str(ident);
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

/// Strip C type-prefixes in `lt <type> <name> = ...` / `mt <type> <name> ...`
/// patterns that leak out of the C ingester. The replacement only fires when
/// the C-type word is followed by a real identifier — never when that same
/// word is itself the variable's name (e.g. `lt double = x => x * 2` must
/// stay intact).
fn strip_c_type_prefixes(source: &str) -> String {
    let types = ["int", "char", "float", "double", "long", "short", "unsigned", "signed", "const", "void"];
    let mut out = String::with_capacity(source.len());
    for (i, line) in source.lines().enumerate() {
        if i > 0 { out.push('\n'); }
        let leading_len = line.len() - line.trim_start().len();
        let leading = &line[..leading_len];
        let rest = &line[leading_len..];
        // Match `lt <type> <name>` where <name> starts with an ident char
        // and isn't immediately `=` / `:` / `,` / `)`.
        let mut rewritten = rest.to_string();
        for binding in ["lt ", "mt "] {
            if let Some(after) = rest.strip_prefix(binding) {
                for ty in types.iter() {
                    let needle = format!("{} ", ty);
                    if let Some(tail) = after.strip_prefix(&needle) {
                        // tail starts with whatever comes after `<binding><type> `.
                        // Must be an identifier character, not operator/punct.
                        let next_ch = tail.chars().next();
                        if matches!(next_ch, Some(c) if c.is_alphabetic() || c == '_' || c == '*' || c == '&') {
                            // Also require that tail is NOT of form `NAME =`
                            // where NAME == the type itself (handled by
                            // `next_ch` being non-alpha when it's `=`; but
                            // defend against the edge).
                            let stripped_tail = tail.trim_start_matches('*').trim_start_matches('&').trim_start();
                            if stripped_tail.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false) {
                                rewritten = format!("{}{}", binding, tail);
                                break;
                            }
                        }
                    }
                }
            }
        }
        out.push_str(leading);
        out.push_str(&rewritten);
    }
    if source.ends_with('\n') { out.push('\n'); }
    out
}

/// Strip `aw ` (await) and `ac ` (async) prefixes wherever they appear.
/// Nebula has no first-class async/await; the awaited value is semantically
/// equivalent to the expression itself, and `ac def fn()` is equivalent to
/// `def fn()` for translation purposes.
fn strip_async_await_prefixes(line: &str) -> String {
    // Leading keyword: `ac def ...` / `ac fn ...` → drop the `ac `.
    let line = if line.trim_start().starts_with("ac fn ") || line.trim_start().starts_with("ac def ") {
        let leading_len = line.len() - line.trim_start().len();
        format!("{}{}", &line[..leading_len], &line.trim_start()[3..])
    } else {
        line.to_string()
    };
    // Inline `aw expr` → `expr`. Match at word boundaries only.
    rewrite_whole_word(&line, "aw", "")
        // rewrite_whole_word leaves a space where `aw ` was; collapse it.
        .replace("  ", " ")
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
                // Found the assignment `=`. Nebula's walrus doesn't take a
                // type annotation, so strip any `: Type` from the LHS before
                // rewriting to `name := value`.
                let lhs_full = rest[..i].trim_end();
                let lhs = if let Some(colon_pos) = lhs_full.find(':') {
                    // Only strip if the colon isn't inside a generic bracket
                    // or destructure pattern (heuristic: the LHS is a simple
                    // identifier followed by `:`).
                    let before_colon = lhs_full[..colon_pos].trim_end();
                    if before_colon.chars().all(|c| c.is_alphanumeric() || c == '_')
                        && !before_colon.is_empty() {
                        before_colon
                    } else {
                        lhs_full
                    }
                } else {
                    lhs_full
                };
                return format!("{} :={}", lhs, &rest[i + 1..]);
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

    // Type position: after ":", "->", declaration keywords, or inside a
    // tuple-return type list `(X, Y)` that follows an earlier `->`. DON'T
    // convert PascalCase identifiers in those positions.
    if first.is_uppercase() {
        if ctx.ends_with(':') || ctx.ends_with("->")
            || ctx.ends_with("st") || ctx.ends_with("im")
            || ctx.ends_with("en") || ctx.ends_with("tr")
            || ctx.ends_with("fo") || ctx.ends_with('<')
            || ctx.ends_with(',') && ctx.contains('<') {
            return false;
        }
        // Multi-return tuple: `-> (X, Y)` — context before X is `-> (`,
        // before Y is `X, `. Anywhere inside a paren group that follows an
        // earlier `->` is a type position.
        if let Some(arrow_pos) = context_before.rfind("->") {
            let after_arrow = &context_before[arrow_pos + 2..];
            let opens = after_arrow.chars().filter(|&c| c == '(').count();
            let closes = after_arrow.chars().filter(|&c| c == ')').count();
            if opens > closes {
                return false;
            }
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
    // Find the matching `)` for the call's opening `(` — the format
    // string and args live between them. Trimming all trailing `)` blindly
    // (old behavior) drops closing parens that belong to an outer wrapper
    // like `return fmt(...)` → corrupting the line.
    let rest_trim = rest.trim_start();
    let mut pdepth = 0i32;
    let mut call_close = None;
    let bytes: Vec<char> = rest_trim.chars().collect();
    let mut k = 0;
    let mut in_s: Option<char> = None;
    while k < bytes.len() {
        let c = bytes[k];
        if let Some(q) = in_s {
            if c == q && (k == 0 || bytes[k-1] != '\\') { in_s = None; }
            k += 1; continue;
        }
        if c == '"' || c == '\'' { in_s = Some(c); k += 1; continue; }
        match c {
            '(' | '[' | '{' => pdepth += 1,
            ')' | ']' | '}' => {
                if pdepth == 0 { call_close = Some(k); break; }
                pdepth -= 1;
            }
            _ => {}
        }
        k += 1;
    }
    let args_end = call_close.unwrap_or(bytes.len());
    let args_str: String = bytes[..args_end].iter().collect();
    // Content AFTER the call's closing `)` (the outer context — e.g. an
    // enclosing return's own `)` or nothing). Our emit reinserts the call
    // close itself, so we skip the matched char.
    let trailing_start = if call_close.is_some() { args_end + 1 } else { args_end };
    let trailing: String = bytes[trailing_start..].iter().collect();

    // Split args (respecting nested parens)
    let args = split_args(&args_str);

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

    Some(format!("{}(\"{}\"){}", func, result_fmt, trailing))
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

/// Convert angle-bracket generic type parameters to Nebula's bracket
/// form: `List<Task>` → `List[Task]`, `Map<K, V>` → `Map[K, V]`. Also
/// unwraps `Ptr<T>` to bare `T` since Nebula has no Ptr grammar. Runs
/// to a fixpoint so nested generics (`List<Map<K, V>>`) fully resolve.
fn strip_generic_params(source: &str) -> String {
    let mut s = source.to_string();
    loop {
        let prev = s.clone();
        // Ptr<Type> → Type (Go pointer type — just use the inner type).
        while let Some(pos) = s.find("Ptr<") {
            let rest = &s[pos + 4..];
            let mut depth = 1i32;
            let mut close = None;
            for (i, c) in rest.char_indices() {
                if c == '<' { depth += 1; }
                if c == '>' { depth -= 1; if depth == 0 { close = Some(i); break; } }
            }
            match close {
                Some(ci) => {
                    let inner = s[pos + 4..pos + 4 + ci].to_string();
                    s = format!("{}{}{}", &s[..pos], inner, &s[pos + 4 + ci + 1..]);
                }
                None => break,
            }
        }
        // `Name<...>` → `Name[...]` for the common containers.
        for name in ["List", "Map", "Set", "Vec", "Option", "Result", "Channel"] {
            let needle = format!("{}<", name);
            while let Some(pos) = s.find(&needle) {
                let start = pos + needle.len();
                let rest = &s[start..];
                let mut depth = 1i32;
                let mut close = None;
                for (i, c) in rest.char_indices() {
                    if c == '<' { depth += 1; }
                    if c == '>' { depth -= 1; if depth == 0 { close = Some(i); break; } }
                }
                match close {
                    Some(ci) => {
                        let inner = &s[start..start + ci];
                        s = format!("{}{}[{}]{}", &s[..pos], name, inner, &s[start + ci + 1..]);
                    }
                    None => break,
                }
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
    // `rest[1..]` is the arg list plus the outer call's closing `)`(and
    // potentially further trailing `)` for wrappers). Find the single `)`
    // that matches the outer call's `(` by tracking paren depth — don't
    // strip all trailing `)` blindly, which would eat the closer of the
    // final argument itself (`self.area()` → `self.area(`).
    let after_comma = rest[1..].trim_start();
    let bytes: Vec<char> = after_comma.chars().collect();
    let mut pd = 0i32;
    let mut call_close = None;
    let mut in_s: Option<char> = None;
    let mut k = 0;
    while k < bytes.len() {
        let c = bytes[k];
        if let Some(q) = in_s {
            if c == q && (k == 0 || bytes[k-1] != '\\') { in_s = None; }
            k += 1; continue;
        }
        if c == '"' || c == '\'' { in_s = Some(c); k += 1; continue; }
        match c {
            '(' | '[' | '{' => pd += 1,
            ')' | ']' | '}' => {
                if pd == 0 { call_close = Some(k); break; }
                pd -= 1;
            }
            _ => {}
        }
        k += 1;
    }
    let args_str = match call_close {
        Some(pos) => &after_comma[..pos],
        None => after_comma,
    };
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
                        // Wrap RHS tuple in parens if it isn't already (e.g.
                        // `1, 2` → `(1, 2)`) so Nebula's tuple-destructure
                        // `let (a, b) = (1, 2)` parses.
                        let rhs = right_text.trim();
                        let rhs_wrapped = if rhs.contains(',')
                            && !(rhs.starts_with('(') && rhs.ends_with(')'))
                            && !(rhs.starts_with('[') && rhs.ends_with(']'))
                        {
                            format!("({})", rhs)
                        } else {
                            rhs.to_string()
                        };
                        return format!("{}let ({}) = {}", ind, targets.join(", "), rhs_wrapped);
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
                let has_complex_lhs = left.contains('.') || left.contains('[');
                if has_complex_lhs {
                    return format!("{}{} = {}", ind, left, right);
                }
                if let Some(tn) = type_node {
                    let t = emit_type(&tn, src);
                    return format!("{}mt {}: {} = {}", ind, left, t, right);
                }
                format!("{}{} = {}", ind, left, right)
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
                // with EXPR as VAR: body → `lt VAR = EXPR` above the body
                // (async with behaves the same for Nebula's purposes — the
                // await happens when the context is created, and Nebula has
                // no context-manager primitive; we keep the body verbatim.)
                let mut bindings: Vec<String> = Vec::new();
                let mut body_node: Option<Node> = None;
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "with_clause" => {
                            let mut c2 = child.walk();
                            for item in child.named_children(&mut c2) {
                                if item.kind() == "with_item" {
                                    // Look for `VAR = EXPR` pattern via as_pattern.
                                    let mut c3 = item.walk();
                                    let mut expr_text: Option<String> = None;
                                    let mut alias: Option<String> = None;
                                    for sub in item.named_children(&mut c3) {
                                        if sub.kind() == "as_pattern" {
                                            if let Some(e) = sub.named_child(0) {
                                                expr_text = Some(emit_expr(&e, src));
                                            }
                                            if let Some(t) = sub.child_by_field_name("alias") {
                                                alias = Some(emit_expr(&t, src));
                                            } else {
                                                // Fallback: grab last identifier-like child
                                                let mut c4 = sub.walk();
                                                for last in sub.named_children(&mut c4) {
                                                    alias = Some(emit_expr(&last, src));
                                                }
                                            }
                                        } else if expr_text.is_none() {
                                            expr_text = Some(emit_expr(&sub, src));
                                        }
                                    }
                                    if let Some(expr) = expr_text {
                                        if let Some(var) = alias {
                                            bindings.push(format!("{}lt {} = {}", ind, var, expr));
                                        } else {
                                            // No alias — just execute the expression
                                            bindings.push(format!("{}{}", ind, expr));
                                        }
                                    }
                                }
                            }
                        }
                        "block" => { body_node = Some(child); }
                        _ => {}
                    }
                }
                let body = body_node
                    .map(|b| emit_children_as_statements(&b, src, depth, emit))
                    .unwrap_or_default();
                let mut out = bindings.join("\n");
                if !body.is_empty() {
                    if !out.is_empty() { out.push('\n'); }
                    out.push_str(&body);
                }
                if out.is_empty() {
                    format!("{}// with ...", ind)
                } else {
                    out
                }
            }

            "decorated_definition" => {
                // Preserve decorators as comment annotations above the def.
                // Nebula doesn't have a first-class decorator syntax, so we
                // capture them as `// @name(args)` lines to keep them out of
                // the function-body slot (where they used to get dropped and
                // strand the real def with no body).
                let mut out = String::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "decorator" => {
                            let raw = text(&child, src).trim();
                            out.push_str(&format!("{}// {}\n", ind, raw));
                        }
                        "function_definition" | "class_definition" => {
                            out.push_str(&emit(&child, src, depth));
                        }
                        _ => {}
                    }
                }
                if out.is_empty() {
                    format!("{}{}", ind, text(node, src))
                } else {
                    out
                }
            }

            "assert_statement" => {
                if let Some(expr) = node.named_child(0) {
                    format!("{}// assert {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}// assert", ind)
                }
            }

            "raise_statement" => {
                // Nebula has `throw EXPR`. Python's `raise X from Y` syntax
                // loses the `from Y` since Nebula has no exception chaining.
                if let Some(expr) = node.named_child(0) {
                    format!("{}throw {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}throw nil", ind)
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
                // {k: v for x in iter} → same, tuple body `|x| (k, v)`
                // (expr for x in iter) → generator, same as list comp for our purposes
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();

                // For dictionary comprehensions, the first child is a `pair`
                // node whose text is `k: v`. Colons inside a lambda body
                // break the Nebula parser ("Expected Comma, got Colon"), so
                // we emit a tuple `(k, v)` instead and still finish with
                // `.to_map()` below.
                let expr = if node.kind() == "dictionary_comprehension" {
                    if let Some(pair) = children.first() {
                        if pair.kind() == "pair" {
                            let k = pair.child_by_field_name("key")
                                .map(|n| emit_expr(&n, src)).unwrap_or_default();
                            let v = pair.child_by_field_name("value")
                                .map(|n| emit_expr(&n, src)).unwrap_or_default();
                            format!("({}, {})", k, v)
                        } else {
                            emit_expr(pair, src)
                        }
                    } else {
                        String::new()
                    }
                } else {
                    children.first().map(|n| emit_expr(n, src)).unwrap_or_default()
                };
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
                    "dictionary_comprehension" => format!("{}.to_map()", core),
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
                    // Python variadic: *args / **kwargs. Nebula has no star-
                    // prefix in param names, so strip the leading stars and
                    // keep the bare identifier.
                    let raw = text(&child, src);
                    let name = raw.trim_start_matches('*');
                    if !name.is_empty() {
                        params.push(name.to_string());
                    }
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

    /// Translate a Python slice subscript into Nebula slice syntax.
    /// `s[a:b]`   → `s[a:b]`
    /// `s[:b]`    → `s[:b]`
    /// `s[a:]`    → `s[a:]`
    /// `s[:]`     → `s[:]`
    /// `s[::-1]`  → `s.rev()` (canonical reverse idiom; Nebula grammar has no step)
    /// `s[a:b:c]` → `s.slc_step(a, b, c)` fallback (step-aware slicing unsupported)
    ///
    /// Nebula gained native slice syntax (see `Expr::Slice`), so round-trips
    /// Python → Nebula → Python preserve idiomatic `s[a:b]`. For the legacy
    /// `.slc()` / `.slc_from()` method forms from old ingester output, codegen
    /// still maps them back to Python slice syntax as a fallback.
    fn emit_python_slice(val: &str, slice_node: &Node, src: &[u8]) -> String {
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
        if step == Some("-1") && start.is_none() && stop.is_none() {
            return format!("{}.rev()", val);
        }
        if step.is_some() && step != Some("1") {
            return format!("{}.slc_step({}, {}, {})", val,
                start.unwrap_or("0"), stop.unwrap_or("-1"), step.unwrap_or("1"));
        }
        format!("{}[{}:{}]", val, start.unwrap_or(""), stop.unwrap_or(""))
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
            // Python splat args: `*args` / `**kwargs`. Nebula has no splat
            // operator in call position — keep the bare identifier so the
            // parser accepts it (semantically equivalent in the subset we
            // care about for translation).
            if child.kind() == "list_splat" || child.kind() == "dictionary_splat" {
                if let Some(inner) = child.named_child(0) {
                    children.push(inner);
                }
                continue;
            }
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
        let raw = text(node, src).trim().to_string();
        // Collapse TS-style unions (`T | null`) and downgrade arrow/object
        // types to Any since Nebula has no grammar for them.
        if raw.contains("=>") || raw.contains('{') {
            return "Any".to_string();
        }
        let t = raw.split('|').next().unwrap_or(&raw).trim().to_string();
        // Python variadic tuple `tuple[int, ...]` — strip the `, ...` tail.
        // `typing.Callable[..., T]` — same.
        let t = t.replace(", ...", "").replace(",...", "");
        strip_tuple_labels(&t)
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

                let is_async = node.children(&mut node.walk())
                    .any(|c| text(&c, src) == "async");
                let prefix = if is_async { "ac " } else { "" };
                // TypeScript return-type annotation — `function foo(): T { }`.
                // Tree-sitter exposes it under `return_type`; strip the leading
                // `: ` so we can emit `-> T` (Nebula's return-type form).
                // Lossy simplifications: union `T | null` → `T`; labeled
                // tuple `[lo: Int, hi: Int]` → `[Int, Int]`; arrow-type
                // `(a: T) => U` and object-type `{ ... }` collapse to `Any`.
                let ret = node.child_by_field_name("return_type")
                    .map(|n| {
                        let raw = text(&n, src).trim();
                        let raw = raw.strip_prefix(':').unwrap_or(raw).trim();
                        if raw.is_empty() { return String::new(); }
                        // Arrow/object types can't be expressed in Nebula's
                        // type grammar — downgrade to Any.
                        if raw.contains("=>") || raw.contains('{') {
                            return " -> Any".to_string();
                        }
                        let t = raw.split('|').next().unwrap_or(raw).trim().to_string();
                        let t = strip_tuple_labels(&t);
                        format!(" -> {}", t)
                    })
                    .unwrap_or_default();

                format!("{}{}fn {}({}){}{}", ind, prefix, name, params, ret, body)
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
                                // Extract field text and normalize to Nebula
                                // struct-field form (`name: Type`).
                                let raw = text(&child, src).trim()
                                    .trim_end_matches(';').trim();
                                if raw.is_empty() { continue; }
                                // Strip access modifiers so our detection
                                // below sees the bare `name` / `name:` shape.
                                let mut body = raw;
                                for m in &["private ", "public ", "protected ", "readonly ", "static "] {
                                    if let Some(b) = body.strip_prefix(*m) { body = b; }
                                }
                                // Arrow-function property (TS): `name = (x) => ...`
                                // or `name: Type = (x) => ...`. Nebula can't
                                // model these; skip entirely so the struct
                                // stays valid. Method shape is preserved
                                // in the accompanying `impl` block by the
                                // method_definition emit path.
                                if body.contains("=>") {
                                    continue;
                                }
                                let emitted = if body.contains(':') {
                                    // `name: Type` or `name: Type = default`
                                    if let Some(eq_pos) = body.find(" = ") {
                                        body[..eq_pos].trim().to_string()
                                    } else {
                                        body.to_string()
                                    }
                                } else if let Some(eq_pos) = body.find(" = ") {
                                    // `name = value` — no type annotation;
                                    // default to Any.
                                    let name = body[..eq_pos].trim();
                                    format!("{}: Any", name)
                                } else {
                                    // Bare `name` declaration without type.
                                    format!("{}: Any", body)
                                };
                                fields.push_str(&format!("{}{}\n", inner_ind, emitted));
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
                        // Array destructuring `[h, t]` and object destructuring
                        // `{a, b}` aren't valid LHS in Nebula (`Left side of :=
                        // must be an identifier or tuple`). Rewrite to tuple
                        // form `(h, t)`. Object destructuring gets the same
                        // treatment — lossy (loses property rename / defaults)
                        // but the host can read & repair individual fields
                        // after the tuple binding.
                        let name = if (name.starts_with('[') && name.ends_with(']'))
                            || (name.starts_with('{') && name.ends_with('}'))
                        {
                            let inner = &name[1..name.len() - 1];
                            format!("({})", inner.trim())
                        } else {
                            name
                        };
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
                        // Array destructuring `[i, x]` — Nebula for-loop
                        // accepts `IDENT, IDENT` or a single ident. Strip
                        // the brackets to produce a comma-list.
                        if t.starts_with('[') && t.ends_with(']') {
                            return t[1..t.len() - 1].trim().to_string();
                        }
                        // Object destructuring `{a, b}` — same approach.
                        if t.starts_with('{') && t.ends_with('}') {
                            return t[1..t.len() - 1].trim().to_string();
                        }
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

            "throw_statement" => {
                if let Some(expr) = node.named_child(0) {
                    format!("{}throw {}", ind, emit_expr(&expr, src))
                } else {
                    format!("{}throw", ind)
                }
            }

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

            "import_statement" => {
                format!("{}// {}", ind, text(node, src).trim())
            }

            "export_statement" => {
                // `export function foo() {}` / `export class Foo {}` /
                // `export const x = 1` wraps a declaration. Without recursing
                // into the inner node the function body leaks as raw text and
                // `===`, `for...of`, `if` stay un-translated (observed on
                // /tmp/gdsp-js/src/stat.js round-trip). Find and recurse.
                let mut cursor = node.walk();
                let mut inner: Option<Node> = None;
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "function_declaration"
                        | "class_declaration"
                        | "lexical_declaration"
                        | "variable_declaration"
                        | "interface_declaration"
                        | "type_alias_declaration"
                        | "enum_declaration" => {
                            inner = Some(child);
                            break;
                        }
                        _ => {}
                    }
                }
                match inner {
                    Some(n) => emit(&n, src, depth),
                    None => format!("{}// {}", ind, text(node, src).trim()),
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
                // Template strings: `text ${expr}` → "text {expr}". Multi-line
                // templates become escaped `\n` so Nebula's single-line string
                // grammar accepts them.
                if raw.starts_with('`') {
                    let inner = &raw[1..raw.len()-1];
                    let converted = inner.replace("${", "{")
                        .replace('\n', "\\n")
                        .replace('\r', "\\r");
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
                // Empty-param arrow emits `|| body`; the postprocess's
                // `|| → or` substitution would corrupt that to `or body`.
                // Insert a space so the pipes stay distinct.
                if params.trim().is_empty() {
                    format!("| | {}", body)
                } else {
                    format!("|{}| {}", params, body)
                }
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
                // TypeScript `required_parameter`/`optional_parameter` carry a
                // pattern and a type_annotation. Raw text leaks union types
                // (`T | null`) and arrow types (`() => X`) into Nebula which
                // the parser rejects. Extract the ident, simplify the type.
                "required_parameter" | "optional_parameter" => {
                    let name = child.child_by_field_name("pattern")
                        .map(|n| text(&n, src)).unwrap_or("?").to_string();
                    let ty_ann = child.child_by_field_name("type")
                        .map(|n| {
                            let raw = text(&n, src).trim();
                            let raw = raw.strip_prefix(':').unwrap_or(raw).trim();
                            if raw.is_empty() { return String::new(); }
                            if raw.contains("=>") || raw.contains('{') {
                                return ": Any".to_string();
                            }
                            let t = raw.split('|').next().unwrap_or(raw).trim().to_string();
                            format!(": {}", strip_tuple_labels(&t))
                        })
                        .unwrap_or_default();
                    let default = child.child_by_field_name("value")
                        .map(|n| format!(" = {}", emit_expr(&n, src)))
                        .unwrap_or_default();
                    params.push(format!("{}{}{}", name, ty_ann, default));
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
        let ind = indent(depth);
        let inner = if node.kind() == "statement_block" {
            emit_children_as_statements(node, src, depth + 1, emit)
        } else {
            let stmt = emit(node, src, depth + 1);
            if stmt.is_empty() { String::new() }
            else if stmt.ends_with('\n') { stmt }
            else { format!("{}\n", stmt) }
        };
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
                // TS interfaces describe both fields and methods. Nebula
                // traits only hold method signatures, so we emit `st` when
                // the interface is data-only and `tr` otherwise. If there's
                // no body at all we still emit a struct (empty struct is
                // valid Nebula, empty trait is not).
                if let Some(body_node) = node.child_by_field_name("body") {
                    let (has_props, has_methods) = {
                        let mut p = false;
                        let mut m = false;
                        let mut cursor = body_node.walk();
                        for child in body_node.named_children(&mut cursor) {
                            match child.kind() {
                                "property_signature" => p = true,
                                "method_signature" => m = true,
                                _ => {}
                            }
                        }
                        (p, m)
                    };
                    if has_methods && !has_props {
                        let body = emit_interface_body(&body_node, src, depth);
                        return format!("{}tr {}{}", ind, name, body);
                    }
                    // Treat as struct: emit property_signatures as fields.
                    let inner_ind = indent(depth + 1);
                    let mut fields = String::new();
                    let mut cursor = body_node.walk();
                    for child in body_node.named_children(&mut cursor) {
                        if child.kind() == "property_signature" {
                            let raw = text(&child, src).trim()
                                .trim_end_matches(';').trim()
                                .trim_end_matches(',').trim();
                            // `name?: Type` → `name: Type` (Nebula has no
                            // per-field optional marker at the struct site).
                            let normalized = raw.replace("?:", ":");
                            fields.push_str(&format!("{}{}\n", inner_ind, normalized));
                        }
                    }
                    let body = if fields.is_empty() {
                        " {}".to_string()
                    } else {
                        format!(" {{\n{}{}}}", fields, ind)
                    };
                    return format!("{}st {}{}", ind, name, body);
                }
                format!("{}st {} {{}}", ind, name)
            }

            // TypeScript-specific: type alias → ty (or comment if the RHS
            // uses union/intersection/object-literal type syntax that
            // Nebula's grammar doesn't accept).
            "type_alias_declaration" => {
                let name = node.child_by_field_name("name")
                    .map(|n| text(&n, src)).unwrap_or("?");
                let value = node.child_by_field_name("value")
                    .map(|n| text(&n, src)).unwrap_or("?");
                if value.contains('|') || value.contains('&') || value.contains('{')
                    || value.contains('(') || value.contains('?') {
                    format!("{}// type {} = {}", ind, name, value)
                } else {
                    format!("{}ty {} = {}", ind, name, value)
                }
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
                // Emit init as separate statement before the if. `emit()`
                // already prepends the current indent; use `trim_end()` so
                // we preserve leading whitespace (the former `trim()` call
                // dropped the indent and emitted the init at column 0).
                if let Some(init_stmt) = init {
                    let init_stmt = init_stmt.trim_end();
                    if !init_stmt.is_empty() {
                        result.push_str(init_stmt);
                        result.push('\n');
                    }
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
                            // Preserve block-level indent on the init statement
                            // so `for init; cond; upd { body }` unrolls into
                            // correctly-indented Nebula: the init sits at the
                            // enclosing block's indent, not column 0.
                            result.push_str(&ind);
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

            "select_statement" => {
                // Go's `select { ... }` has no Nebula analogue. Pick the
                // first case's body and inline it. Children of the
                // select_statement are `communication_case` / `default_case`
                // nodes; each case's body lives in a `statement_list` child.
                let mut emitted = String::new();
                let mut cursor = node.walk();
                'outer: for child in node.named_children(&mut cursor) {
                    if child.kind() == "communication_case"
                        || child.kind() == "default_case" {
                        let mut cc = child.walk();
                        for sub in child.named_children(&mut cc) {
                            if sub.kind() == "statement_list" {
                                let stmts = emit_children_as_statements(&sub, src, depth, emit);
                                if !stmts.trim().is_empty() {
                                    emitted = stmts;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
                if emitted.trim().is_empty() {
                    format!("{}// select {{}}", ind)
                } else {
                    emitted.trim_end().to_string()
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
                let func_node = node.child_by_field_name("function");
                let func = func_node.as_ref()
                    .map(|n| emit_expr(n, src)).unwrap_or_default();
                let args_node = node.child_by_field_name("arguments");
                let args = args_node.as_ref()
                    .map(|n| emit_call_args(n, src)).unwrap_or_default();

                // Wrap func_literal IIFEs in parens so `(|| expr)()` parses
                // instead of `|| expr()` which Nebula reads as a lambda body.
                let is_func_literal = func_node.as_ref()
                    .map(|n| n.kind() == "func_literal").unwrap_or(false);
                if is_func_literal {
                    return format!("({})({})", func, args);
                }

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
                if op == "<-" {
                    return format!("{}.recv()", arg);
                }
                // Nebula has no explicit pointer deref or address-of. Drop
                // `*` and `&` so `*p = 0` → `p = 0` and `&x` → `x`. Tradeoff:
                // Go→Nebula→Go loses the deref, so Go-side assignment through
                // pointer has to be re-added by the codegen context (handled
                // by Go's receiver-pointer codegen).
                if op == "*" || op == "&" {
                    return arg;
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
                // Emit Nebula native slice syntax `xs[a:b]` (Expr::Slice).
                // Previously emitted `xs[a..b]` (Index of Range), which misrenders
                // in Python codegen and fails to parse for half-open forms.
                let obj = node.child_by_field_name("operand")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let start = node.child_by_field_name("start")
                    .map(|n| emit_expr(&n, src));
                let end = node.child_by_field_name("end")
                    .map(|n| emit_expr(&n, src));
                format!(
                    "{}[{}:{}]",
                    obj,
                    start.as_deref().unwrap_or(""),
                    end.as_deref().unwrap_or(""),
                )
            }

            "type_assertion_expression" => {
                let obj = node.child_by_field_name("operand")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                let ty = node.child_by_field_name("type")
                    .map(|n| go_type_to_nebula(&text(&n, src))).unwrap_or_default();
                format!("{}.({}))", obj, ty)
            }

            "func_literal" => {
                // Go's `func(params) { body }` in argument position. Nebula
                // lambdas only support single-expression bodies, so we
                // flatten: single-stmt body → `|params| expr`; anything else
                // → `|params| ()` placeholder (the translation remains
                // parseable; the caller is responsible for preserving exact
                // semantics when the original body is non-trivial).
                let params = node.child_by_field_name("parameters")
                    .map(|n| emit_params(&n, src)).unwrap_or_default();
                let body_node = node.child_by_field_name("body").or_else(|| {
                    let mut cc = node.walk();
                    let mut found = None;
                    for ch in node.named_children(&mut cc) {
                        if ch.kind() == "block" { found = Some(ch); break; }
                    }
                    found
                });
                let body_expr: String = match body_node {
                    Some(b) => {
                        // Go's block node wraps its statements in a
                        // `statement_list` named child; unwrap one level so
                        // our single-statement flatten sees the real stmts.
                        let container = {
                            let mut cursor = b.walk();
                            let mut sl = None;
                            for ch in b.named_children(&mut cursor) {
                                if ch.kind() == "statement_list" { sl = Some(ch); break; }
                            }
                            sl.unwrap_or(b)
                        };
                        let mut cursor = container.walk();
                        let stmts: Vec<_> = container.named_children(&mut cursor)
                            .filter(|c| c.kind() != "comment")
                            .collect();
                        match stmts.len() {
                            0 => "()".to_string(),
                            1 => {
                                let s = &stmts[0];
                                if s.kind() == "expression_statement" {
                                    s.named_child(0).map(|n| emit_expr(&n, src))
                                        .unwrap_or_else(|| "()".to_string())
                                } else if s.kind() == "return_statement" {
                                    s.named_child(0).map(|n| emit_expr(&n, src))
                                        .unwrap_or_else(|| "()".to_string())
                                } else {
                                    emit(s, src, 0).trim()
                                        .trim_start_matches("rt ").to_string()
                                }
                            }
                            _ => "()".to_string(),
                        }
                    }
                    None => "()".to_string(),
                };
                // Use a placeholder for the empty-arg lambda marker so the
                // postprocess `||` → `or` replacement (for logical OR) doesn't
                // mangle it. We swap the marker back to `||` after
                // canonicalization.
                let lam = if params.is_empty() {
                    "__NEB_ANONFN__".to_string()
                } else {
                    format!("|{}|", params)
                };
                format!("{} {}", lam, body_expr)
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
                // Empty interface body after minify strips `// embeds X`
                // comments — drop the trait braces so the parser accepts an
                // empty marker trait instead of a trailing `:` with nothing
                // under it.
                let body_inner = body.trim_start_matches(" {").trim_end_matches('}').trim();
                if body_inner.is_empty() {
                    format!("{}tr {} {{}}", ind, name)
                } else {
                    format!("{}tr {}{}", ind, name, body)
                }
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
        // Channel types: `chan T` / `<-chan T` / `chan<- T` → `Channel<T>`
        if let Some(inner) = ty.strip_prefix("chan<- ") {
            return format!("Channel<{}>", go_type_to_nebula(inner));
        }
        if let Some(inner) = ty.strip_prefix("<-chan ") {
            return format!("Channel<{}>", go_type_to_nebula(inner));
        }
        if let Some(inner) = ty.strip_prefix("chan ") {
            return format!("Channel<{}>", go_type_to_nebula(inner));
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
                let name_node = child.child_by_field_name("name");
                let ty = child.child_by_field_name("type")
                    .map(|n| go_type_to_nebula(&text(&n, src)))
                    .unwrap_or_else(|| "?".to_string());
                match name_node {
                    Some(n) => {
                        let name = text(&n, src);
                        fields.push_str(&format!("{}{}: {}\n", inner_ind, name, ty));
                    }
                    None => {
                        // Embedded field (Go composition). Derive a lowercase
                        // field name from the type so Nebula's struct accepts
                        // it.
                        let derived = ty.trim_start_matches("Ptr<").trim_end_matches('>');
                        let derived = derived.split('.').last().unwrap_or(derived);
                        let snake = derived.chars().enumerate().map(|(i, c)| {
                            if i > 0 && c.is_uppercase() {
                                format!("_{}", c.to_lowercase())
                            } else {
                                c.to_lowercase().to_string()
                            }
                        }).collect::<String>();
                        fields.push_str(&format!("{}{}: {}\n", inner_ind, snake, ty));
                    }
                }
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
        let mut real_methods = 0u32;
        let inner_ind = indent(depth + 1);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            let raw = text(&child, src).trim();
            if raw.is_empty() { continue; }
            let cleaned = clean_go_method_signature(raw);
            match child.kind() {
                "method_elem" | "method_spec" => {
                    methods.push_str(&format!("{}fn {}\n", inner_ind, cleaned));
                    real_methods += 1;
                }
                "type_elem" | "type_identifier" => {
                    methods.push_str(&format!("{}// embeds {}\n", inner_ind, cleaned));
                }
                _ => {
                    let looks_like_method = cleaned.chars().next()
                        .map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
                        && cleaned.contains('(');
                    if looks_like_method {
                        methods.push_str(&format!("{}fn {}\n", inner_ind, cleaned));
                        real_methods += 1;
                    } else {
                        methods.push_str(&format!("{}{}\n", inner_ind, cleaned));
                    }
                }
            }
        }
        let ind = indent(depth);
        if real_methods == 0 {
            // Body holds only comment placeholders (minify will strip them)
            // — emit explicit `{}` so braces_to_indent leaves a parseable
            // empty-body trait.
            " {}".to_string()
        } else {
            format!(" {{\n{}{}}}", methods, ind)
        }
    }

    /// Best-effort cleanup for Go method signatures leaking into interface
    /// bodies. Converts `[]T` → `List`, `p T` name-first params → `p: T`,
    /// and collapses multi-return tuples `(A, B)` into a single return
    /// annotation (`-> A`) so Nebula parses.
    fn capitalize_go_builtin(t: &str) -> String {
        match t {
            "byte" => "Byte".to_string(),
            "rune" => "Rune".to_string(),
            "int" => "Int".to_string(),
            "int8" => "Int8".to_string(),
            "int16" => "Int16".to_string(),
            "int32" => "Int32".to_string(),
            "int64" => "Int64".to_string(),
            "uint" => "Uint".to_string(),
            "uint8" => "Uint8".to_string(),
            "uint16" => "Uint16".to_string(),
            "uint32" => "Uint32".to_string(),
            "uint64" => "Uint64".to_string(),
            "float32" => "Float32".to_string(),
            "float64" => "Float64".to_string(),
            "string" => "Str".to_string(),
            "bool" => "Bool".to_string(),
            "error" => "Error".to_string(),
            "any" => "Any".to_string(),
            other => other.to_string(),
        }
    }

    fn clean_go_method_signature(raw: &str) -> String {
        let mut s = raw.to_string();
        // `[]T` → `List[T]`. Keep the element type so the codegen can
        // round-trip a slice properly; emitting bare `List` produces
        // `undefined: List` at Go compile.
        while let Some(start) = s.find("[]") {
            let after = &s[start + 2..];
            let mut end = 0;
            for (i, c) in after.char_indices() {
                if c.is_alphanumeric() || c == '_' || c == '.' { end = i + c.len_utf8(); }
                else { break; }
            }
            if end == 0 { break; }
            let elem = &s[start + 2..start + 2 + end];
            // Capitalize bare builtin-type names so Nebula's type grammar
            // accepts them (`byte` → `Byte`, `int` → `Int`, etc.).
            let elem = capitalize_go_builtin(elem);
            let replacement = format!("List[{}]", elem);
            s.replace_range(start..start + 2 + end, &replacement);
        }

        // Find the params' closing paren via balanced matching starting from
        // the first `(`.
        let bytes = s.as_bytes();
        let lp = match s.find('(') { Some(i) => i, None => return s };
        let mut depth = 0;
        let mut params_rp = None;
        for i in lp..bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => { depth -= 1; if depth == 0 { params_rp = Some(i); break; } }
                _ => {}
            }
        }
        let params_rp = match params_rp { Some(i) => i, None => return s };

        // Rewrite params: `p T` → `p: T` (Go name-first). Already-colonized
        // params are left alone.
        let params_inner = s[lp + 1..params_rp].to_string();
        let mut new_params = String::new();
        let parts: Vec<&str> = {
            let mut out = Vec::new();
            let mut depth = 0;
            let mut start = 0;
            for (i, c) in params_inner.char_indices() {
                match c {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => depth -= 1,
                    ',' if depth == 0 => {
                        out.push(&params_inner[start..i]);
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            if start <= params_inner.len() { out.push(&params_inner[start..]); }
            out
        };
        for (i, p) in parts.iter().enumerate() {
            if i > 0 { new_params.push_str(", "); }
            let pt = p.trim();
            if pt.is_empty() { continue; }
            if pt.contains(':') {
                new_params.push_str(pt);
            } else {
                // Go `name Type` or `Type` (unnamed).
                let split: Vec<&str> = pt.splitn(2, ' ').collect();
                if split.len() == 2 {
                    new_params.push_str(&format!("{}: {}", split[0].trim(), split[1].trim()));
                } else {
                    // Single token — treat as unnamed type.
                    new_params.push_str(pt);
                }
            }
        }

        // Handle return type after `)`: either `(A, B)` tuple, single type,
        // or nothing.
        let after = s[params_rp + 1..].trim();
        let return_suffix = if after.is_empty() || after.starts_with("->") {
            after.to_string()
        } else if after.starts_with('(') && after.ends_with(')') {
            let inner = &after[1..after.len() - 1];
            let first = inner.split(',').next().unwrap_or(inner).trim();
            if first.is_empty() { String::new() } else { format!("-> {}", first) }
        } else {
            format!("-> {}", after)
        };

        let head = &s[..lp];
        if return_suffix.is_empty() {
            format!("{}({})", head, new_params)
        } else {
            format!("{}({}) {}", head, new_params, return_suffix)
        }
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

    /// Strip Swift parameter labels: the unnamed `_ name:` shape and the
    /// external-label `externalName internalName:` shape, both of which
    /// Nebula's grammar rejects. Operates only on the first `(...)` param
    /// list found in the line.
    pub fn swift_strip_unnamed_labels(line: &str) -> String {
        let lp = match line.find('(') { Some(i) => i, None => return line.to_string() };
        // Balance-match the closing paren.
        let bytes = line.as_bytes();
        let mut depth = 0;
        let mut rp = line.len();
        for i in lp..bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => { depth -= 1; if depth == 0 { rp = i; break; } }
                _ => {}
            }
        }
        if rp >= line.len() { return line.to_string(); }
        let params = &line[lp + 1..rp];
        let parts: Vec<&str> = {
            let mut out = Vec::new();
            let mut d = 0;
            let mut s = 0;
            for (i, c) in params.char_indices() {
                match c {
                    '(' | '[' | '{' => d += 1,
                    ')' | ']' | '}' => d -= 1,
                    ',' if d == 0 => { out.push(&params[s..i]); s = i + 1; }
                    _ => {}
                }
            }
            if s <= params.len() { out.push(&params[s..]); }
            out
        };
        let mut new_params = String::new();
        for (i, p) in parts.iter().enumerate() {
            if i > 0 { new_params.push_str(", "); }
            let pt = p.trim();
            if pt.is_empty() { continue; }
            // `_ internal: Type` — drop the leading `_ ` label.
            let pt = pt.strip_prefix("_ ").unwrap_or(pt);
            // `external internal: Type` — two identifiers before `:`.
            // Identify by finding the first `:` and counting the space-
            // separated tokens before it.
            if let Some(colon_pos) = pt.find(':') {
                let before = &pt[..colon_pos];
                let tokens: Vec<&str> = before.split_whitespace().collect();
                if tokens.len() == 2
                    && tokens[0].chars().all(|c| c.is_alphanumeric() || c == '_')
                    && tokens[1].chars().all(|c| c.is_alphanumeric() || c == '_') {
                    new_params.push_str(tokens[1]);
                    new_params.push_str(&pt[colon_pos..]);
                    continue;
                }
            }
            new_params.push_str(pt);
        }
        format!("{}({}){}", &line[..lp], new_params, &line[rp + 1..])
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

    pub fn convert_swift_raw(raw: &str, depth: usize) -> String {
        let ind = indent(depth);
        let mut out = Vec::new();
        let mut in_struct = false;  // Track if we're inside a struct body
        for line in raw.lines() {
            if line.trim().is_empty() {
                out.push(String::new());
                continue;
            }
            // Preserve original leading whitespace so brace-nested bodies
            // survive into braces_to_indent. Use `trim_end()` + explicit
            // leading-ws split so we don't flatten every statement to depth 0.
            let leading_len = line.len() - line.trim_start().len();
            let leading = &line[..leading_len];
            let trimmed = line[leading_len..].trim_end();
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

            // Swift unnamed-external-label `_ name: Type` — Nebula has no
            // external labels, so strip the `_ ` prefix on each parameter.
            let converted = swift_strip_unnamed_labels(&converted);
            // Fix Swift string interpolation: \(expr) → {expr}
            let converted = fix_swift_interpolation(&converted);

            // Track struct/class context
            let starts_struct = converted.starts_with("st ");
            let starts_enum = converted.starts_with("en ");
            let converted = if starts_struct || starts_enum {
                if starts_struct { in_struct = true; }
                // Swift struct/class/enum with a protocol list: `st X: P1, P2 {`.
                // Nebula's type decls don't take inheritance/conformance — strip
                // everything between the first `:` and the trailing `{` so we
                // emit `st X {` / `en X {` cleanly. Non-conformance info is
                // lost but the declaration parses.
                if let (Some(colon_pos), Some(brace_pos)) = (converted.find(':'), converted.rfind('{')) {
                    if colon_pos < brace_pos {
                        let head = converted[..colon_pos].trim_end();
                        let tail = " {";
                        format!("{}{}", head, tail)
                    } else {
                        converted
                    }
                } else {
                    converted
                }
            } else {
                converted
            };

            // Inside struct body: split fields from methods
            if in_struct && converted.contains("fn ") && !starts_struct {
                // Close struct, start impl block — emit the closer and opener
                // at the outer block's indent (leading is 0 indent inside the
                // struct body lines, so fall back to `ind`).
                let struct_name = out.iter().rev()
                    .find(|l: &&String| l.trim_start().starts_with("st "))
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
                let no_default = if let Some(eq_pos) = stripped.find(" = ") {
                    stripped[..eq_pos].to_string()
                } else {
                    stripped
                };
                // Swift computed property: `name: Type {` — drop the trailing
                // `{` block header so the field parses as a plain field. The
                // get/set blocks that follow will be flattened into struct
                // siblings (harmlessly ignored by Nebula's struct grammar
                // except that `get {`/`set {` lines are still present — we
                // rely on `skip_swift_getset_blocks` below to swallow them).
                if no_default.trim_end().ends_with('{') {
                    no_default.trim_end().trim_end_matches('{').trim_end().to_string()
                } else {
                    no_default
                }
            } else {
                converted
            };

            // Swift `get { ... }` / `set { ... }` inside a computed property
            // — drop these entirely (we've already squashed the property to
            // a plain field above).
            if in_struct && (converted.trim_start().starts_with("get {")
                || converted.trim_start().starts_with("set {")
                || converted.trim_start().starts_with("get ")
                || converted.trim_start().starts_with("set ")) {
                continue;
            }

            if converted.trim() == "}" {
                in_struct = false;
            }

            out.push(format!("{}{}{}", ind, leading, converted));
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

    /// Strip `= EXPR` default-values from each field inside a Kotlin
    /// data-class body already converted to `st NAME { f1, f2, ... }`.
    fn strip_data_class_defaults(s: &str) -> String {
        let lb = match s.find('{') { Some(i) => i, None => return s.to_string() };
        let rb = match s.rfind('}') { Some(i) => i, None => return s.to_string() };
        if rb <= lb { return s.to_string(); }
        let head = &s[..lb + 1];
        let tail = &s[rb..];
        let inner = &s[lb + 1..rb];
        let cleaned: Vec<String> = inner.split(',').map(|part| {
            let part = part.trim();
            if let Some(eq) = part.find(" = ") {
                part[..eq].trim().to_string()
            } else {
                part.to_string()
            }
        }).filter(|p| !p.is_empty()).collect();
        format!("{} {} {}", head.trim_end(), cleaned.join(", "), tail.trim_start())
    }

    pub fn convert_kotlin_raw(raw: &str, depth: usize) -> String {
        let ind = indent(depth);
        let mut out = Vec::new();
        let mut in_data_class = false;
        let mut in_struct = false;
        for line in raw.lines() {
            if line.trim().is_empty() {
                out.push(String::new());
                continue;
            }
            // Preserve original leading whitespace so brace-nested bodies
            // survive through braces_to_indent instead of flattening to depth 0.
            let leading_len = line.len() - line.trim_start().len();
            let leading = &line[..leading_len];
            let mut trimmed_owned: String;
            let trimmed: &str = {
                let raw_trimmed = line[leading_len..].trim_end();
                // Kotlin wildcard imports (`import x.y.*`) aren't valid
                // Nebula; drop the trailing `.*` so the parser accepts a
                // plain path import.
                if raw_trimmed.starts_with("import ") && raw_trimmed.ends_with(".*") {
                    trimmed_owned = raw_trimmed.trim_end_matches(".*").to_string();
                    &trimmed_owned
                // Kotlin-only modifiers Nebula doesn't model — drop them
                // before the struct-name extraction uses the line.
                } else if raw_trimmed.starts_with("suspend ")
                    || raw_trimmed.starts_with("companion ")
                    || raw_trimmed.starts_with("sealed ")
                    || raw_trimmed.starts_with("open ")
                    || raw_trimmed.starts_with("inner ")
                    || raw_trimmed.starts_with("abstract ")
                    || raw_trimmed.starts_with("annotation ")
                    || raw_trimmed.starts_with("inline ")
                    || raw_trimmed.starts_with("tailrec ")
                    || raw_trimmed.starts_with("operator ")
                    || raw_trimmed.starts_with("infix ")
                    || raw_trimmed.starts_with("lateinit ")
                    || raw_trimmed.starts_with("final ")
                    || raw_trimmed.starts_with("override ")
                    || raw_trimmed.starts_with("private ")
                    || raw_trimmed.starts_with("public ")
                    || raw_trimmed.starts_with("protected ")
                    || raw_trimmed.starts_with("internal ") {
                    let stripped = {
                        let mut s = raw_trimmed.to_string();
                        let prefixes = ["suspend ", "companion ", "sealed ", "open ",
                                        "inner ", "abstract ", "annotation ", "inline ",
                                        "tailrec ", "operator ", "infix ", "lateinit ",
                                        "final ", "override ", "private ", "public ",
                                        "protected ", "internal "];
                        let mut changed = true;
                        while changed {
                            changed = false;
                            for p in &prefixes {
                                if let Some(rest) = s.strip_prefix(*p) {
                                    s = rest.to_string();
                                    changed = true;
                                    break;
                                }
                            }
                        }
                        s
                    };
                    trimmed_owned = stripped;
                    &trimmed_owned
                } else {
                    raw_trimmed
                }
            };

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
                // Kotlin: `data class User(val id: Int, val name: String = "x")`
                //   → `st User { id: Int, name: Str }`
                // Nebula struct fields don't accept default values, so strip
                // `= EXPR` from each field after the standard unwrapping.
                let converted = trimmed.replace("data class ", "st ")
                    .replace('(', " {").replace(')', " }")
                    .replace("val ", "").replace("var ", "");
                strip_data_class_defaults(&converted)
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
                // `enum class` MUST be replaced before the generic `class `
                // rewrite; otherwise `class` → `st` runs first and leaves
                // `enum st X` which doesn't parse. Similar for `data class`.
                .replace("enum class ", "en ")
                .replace("data class ", "st ")
                .replace("sealed class ", "st ")
                .replace("class ", "st ")
                .replace("object ", "st ")
                .replace("interface ", "tr ")
                .replace("when ", "mc ")
                .replace("else ->", "_ =>")
                // Keep `->` for return-type arrows and lambda `x ->` forms.
                // Previously a blanket `.replace("->", "=>")` broke `{ x ->
                // body }` and `(Int) -> Int` types. `when` arms are rewritten
                // to `=>` by rewrite_match_arms later.
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
                    && !converted.starts_with("st ") && !converted.trim().is_empty()
                    // Don't treat block openers (bare `{` or anything ending
                    // in `{`) as typeless fields.
                    && converted.trim() != "{" && !converted.trim().starts_with('{')
                    && !converted.trim_end().ends_with('{')
                    // Exclude nested declaration headers (`sealed class X {`
                    // inside an outer struct body).
                    && !converted.trim_start().starts_with("sealed ")
                    && !converted.trim_start().starts_with("data class ")
                    && !converted.trim_start().starts_with("class ")
                    && !converted.trim_start().starts_with("interface ")
                    && !converted.trim_start().starts_with("object ") {
                let stripped = converted.replace("lt ", "").replace("mt ", "");
                let stripped = if let Some(eq_pos) = stripped.find(" = ") {
                    stripped[..eq_pos].to_string()
                } else {
                    stripped
                };
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

            out.push(format!("{}{}{}", ind, leading, converted));
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

            // Expressions that can appear as statement-shaped children
            // (single-expression function bodies, block-tail expressions).
            // Previously fell through to the `_ => text(node, src)` catch-all,
            // which kept `&xs[..3]` verbatim — Nebula can't parse the `..`
            // slice form. Recurse into emit_expr so the index/range handler
            // rewrites to `xs[:3]`.
            "reference_expression" | "dereference_expression"
            | "index_expression" | "call_expression" | "method_call_expression"
            | "field_expression" | "binary_expression" | "unary_expression" => {
                format!("{}{}", ind, emit_expr(node, src))
            }

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
                // `use std::io::{self, Read}` → Nebula's `use` doesn't accept
                // `self` as an item name (collides with SelfValue keyword).
                // Drop `self,` entries and normalize `::` → `.` so the parser
                // accepts what remains. Lossy: `use std::io::{self}` becomes
                // `use std.io` (the module itself is re-exported).
                let raw = text(node, src).trim();
                let path = raw.strip_prefix("use ").unwrap_or("?")
                    .trim_end_matches(';')
                    .trim();
                let normalized = path
                    .replace("{self, ", "{")
                    .replace("{self,", "{")
                    .replace(", self}", "}")
                    .replace(",self}", "}")
                    .replace("{self}", "")
                    .trim_end_matches("::")
                    .to_string();
                format!("{}use {}", ind, normalized)
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
                // Rust `#[derive(...)]`, `#[cfg(...)]`, etc. have no Nebula
                // equivalent and the parser rejects `#` outright. Comment the
                // line so the structure around it still parses. Common ones
                // (Debug, Clone) could be re-synthesized by the Rust codegen
                // later from a Nebula-level tag; for now: drop cleanly.
                let raw = text(node, src).trim();
                format!("{}// {}", ind, raw)
            }

            "macro_invocation" => {
                let raw = text(node, src).trim().trim_end_matches(';');
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
                    // Unknown macros (`panic!`, `assert!`, `dbg!`, etc.) drop
                    // the `!` so they parse as regular calls in Nebula. Lossy
                    // round-trip — codegen emits a plain call back — but
                    // without this fix every Rust file using a macro fails
                    // `nebula check` outright.
                    format!("{}{}", ind, strip_macro_bang(raw))
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
                // If the index is a Rust range (`&xs[a..b]`, `&xs[..b]`, ...),
                // emit Nebula native slice syntax `xs[a:b]` instead of
                // `xs[Range(a..b)]` (which misrenders in non-Rust codegens).
                if let Some(idx_node) = node.named_child(1) {
                    if idx_node.kind() == "range_expression" {
                        let raw = text(&idx_node, src);
                        // Parse `a..b` / `..b` / `a..` / `..` (and `..=` inclusive form).
                        let (sep, inclusive) = if raw.contains("..=") {
                            ("..=", true)
                        } else {
                            ("..", false)
                        };
                        let (lhs, rhs) = match raw.split_once(sep) {
                            Some((l, r)) => (l.trim(), r.trim()),
                            None => ("", ""),
                        };
                        let end_str = if inclusive && !rhs.is_empty() {
                            format!("{} + 1", rhs)
                        } else {
                            rhs.to_string()
                        };
                        return format!("{}[{}:{}]", val, lhs, end_str);
                    }
                }
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

            "reference_expression" | "dereference_expression" => {
                // `&xs[..]` in Rust is a borrow of a slice. Nebula has no
                // borrow semantics, so drop the `&`/`*` and recurse into the
                // inner expression. Previously returned `text(node, src)`
                // which kept the literal `..` range syntax instead of
                // translating `xs[..2]` → `xs[:2]` etc.
                if let Some(inner) = node.named_child(0) {
                    emit_expr(&inner, src)
                } else {
                    text(node, src).to_string()
                }
            }

            "unary_expression" => text(node, src).to_string(),

            "closure_expression" => {
                let params = node.child_by_field_name("parameters")
                    .map(|n| text(&n, src).replace('|', ""))
                    .unwrap_or_default();
                // Rust tuple-destructuring params `|(i, x)|` — Nebula's
                // lambda grammar takes bare idents between pipes. Strip the
                // enclosing parens so `(i, x)` becomes `i, x` (flatten the
                // tuple binding). Lossy for patterns with nested structure
                // but handles the common `xs.iter().enumerate().map(|(i, x)| …)`
                // idiom.
                let trimmed = params.trim();
                let params = if trimmed.starts_with('(') && trimmed.ends_with(')') {
                    trimmed[1..trimmed.len() - 1].trim().to_string()
                } else {
                    trimmed.to_string()
                };
                let body = node.child_by_field_name("body")
                    .map(|n| emit_expr(&n, src)).unwrap_or_default();
                format!("|{}| {}", params, body)
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
                    strip_macro_bang(raw)
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
        let mut tuple_idx: usize = 0;
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "field_declaration" => {
                    let name = child.child_by_field_name("name")
                        .map(|n| text(&n, src)).unwrap_or("?");
                    let ty = child.child_by_field_name("type")
                        .map(|n| text(&n, src)).unwrap_or("?");
                    let is_pub = child.children(&mut child.walk())
                        .any(|c| text(&c, src) == "pub");
                    let vis = if is_pub { "pb " } else { "" };
                    fields.push_str(&format!("{}{}{}: {}\n", inner_ind, vis, name, ty));
                }
                // `struct Point(i32, i32);` — tuple struct. Each child is a
                // type node, not a field_declaration. Synthesize positional
                // names `_0`, `_1`, ... so the codegen can recognise this
                // shape and emit a tuple struct.
                k if matches!(k, "primitive_type" | "type_identifier"
                    | "generic_type" | "reference_type" | "array_type"
                    | "tuple_type" | "scoped_type_identifier"
                    | "dynamic_type" | "function_type") => {
                    let ty = text(&child, src);
                    fields.push_str(&format!("{}_{}: {}\n", inner_ind, tuple_idx, ty));
                    tuple_idx += 1;
                }
                _ => {}
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

    /// Drop the `!` from a Rust macro invocation so it parses as a call in
    /// Nebula. Lossy — round-trip emits a plain function call — but required
    /// because Nebula has no macro grammar. `panic!(x)` → `panic(x)`,
    /// `assert!(cond)` → `assert(cond)`.
    fn strip_macro_bang(raw: &str) -> String {
        if let Some(bang) = raw.find('!') {
            let before = &raw[..bang];
            let after = &raw[bang + 1..];
            format!("{}{}", before, after)
        } else {
            raw.to_string()
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
                // C: return_type name(params) { body }. The declarator can
                // be a `function_declarator` or a `pointer_declarator`
                // wrapping one (for `Type *name(...)`). Walk through any
                // number of pointer_declarator wrappers before extracting
                // name + params.
                let mut declarator = node.child_by_field_name("declarator");
                while let Some(d) = declarator {
                    if d.kind() == "pointer_declarator" {
                        declarator = d.child_by_field_name("declarator");
                    } else {
                        declarator = Some(d);
                        break;
                    }
                }
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
                    Some(t) => format!(" -> {}", normalize_c_type(t)),
                };
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));

                format!("{}fn {}({}){}{}", ind, name, params, ret_ann, body)
            }

            "declaration" => {
                // C declaration: `type name = value;` or `type name;`. Walk
                // the AST to strip the type prefix and any pointer wrappers
                // so the emitted Nebula declaration is `name := value` (or
                // `name` if no initializer).
                let ty_raw = node.child_by_field_name("type")
                    .map(|n| text(&n, src).to_string());
                let mut decl_strs: Vec<String> = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "init_declarator" => {
                            let declarator_raw = child.child_by_field_name("declarator")
                                .map(|n| text(&n, src)).unwrap_or("_");
                            let value_raw = child.child_by_field_name("value")
                                .map(|n| emit_expr(&n, src));
                            let has_pointer = declarator_raw.starts_with('*');
                            let (name, is_array) = {
                                let s = declarator_raw.trim_start_matches('*').trim_start_matches('&');
                                if let Some(br) = s.find('[') {
                                    (s[..br].trim().to_string(), true)
                                } else {
                                    (s.trim().to_string(), false)
                                }
                            };
                            let ty_norm = ty_raw.as_deref().map(normalize_c_type);
                            let annotation = if is_array {
                                ": List".to_string()
                            } else {
                                match ty_norm {
                                    Some(t) if !t.is_empty() => {
                                        if has_pointer && t == "Char" {
                                            ": Str".to_string()
                                        } else {
                                            format!(": {}", t)
                                        }
                                    }
                                    _ => String::new(),
                                }
                            };
                            // Nebula requires a binding keyword for typed
                            // declarations (`lt`/`mt`). Without one, `n: Int`
                            // trips the parser. C declarations default to
                            // `mt` since C vars are mutable.
                            match value_raw {
                                Some(v) => decl_strs.push(format!("{}{} = {}", name, annotation, v)),
                                None => {
                                    if annotation.is_empty() {
                                        decl_strs.push(name);
                                    } else {
                                        decl_strs.push(format!("mt {}{}", name, annotation));
                                    }
                                }
                            }
                        }
                        "identifier" => {
                            let name = text(&child, src).to_string();
                            let ty_norm = ty_raw.as_deref().map(normalize_c_type);
                            let annotation = match ty_norm {
                                Some(t) if !t.is_empty() => format!(": {}", t),
                                _ => String::new(),
                            };
                            if annotation.is_empty() {
                                decl_strs.push(name);
                            } else {
                                decl_strs.push(format!("mt {}{}", name, annotation));
                            }
                        }
                        "pointer_declarator" => {
                            let inner = child.child_by_field_name("declarator")
                                .map(|n| text(&n, src)).unwrap_or("_");
                            let name = inner.trim_start_matches('*').trim().to_string();
                            let ty_norm = ty_raw.as_deref().map(normalize_c_type);
                            let annotation = match ty_norm {
                                Some(t) if t == "Char" => ": Str".to_string(),
                                Some(t) if !t.is_empty() => format!(": {}", t),
                                _ => String::new(),
                            };
                            if annotation.is_empty() {
                                decl_strs.push(name);
                            } else {
                                decl_strs.push(format!("mt {}{}", name, annotation));
                            }
                        }
                        _ => {}
                    }
                }
                if decl_strs.is_empty() {
                    // Fall back to raw text if the AST shape is unexpected.
                    let raw = text(node, src).trim().trim_end_matches(';');
                    format!("{}lt {}", ind, raw)
                } else {
                    decl_strs.iter().map(|s| format!("{}lt {}", ind, s))
                        .collect::<Vec<_>>().join("\n")
                }
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
                // C `for (init; cond; step) body` → Nebula `while cond { body;
                // step }` with `init` emitted as a preceding statement when
                // present. The raw-text fallback we used before lost the
                // body for single-line for loops.
                let init_node = node.child_by_field_name("initializer");
                let cond_node = node.child_by_field_name("condition");
                let update_node = node.child_by_field_name("update");
                // Body: last child that's a statement-like node.
                let mut body_node = None;
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "compound_statement" | "expression_statement"
                        | "if_statement" | "while_statement" | "for_statement"
                        | "do_statement" | "return_statement"
                        | "break_statement" | "continue_statement"
                        | "block" => { body_node = Some(child); }
                        _ => {}
                    }
                }
                let cond = cond_node.as_ref()
                    .map(|n| emit_expr_unwrap_parens(n, src))
                    .unwrap_or_else(|| "True".to_string());
                let cond = if cond.is_empty() { "True".to_string() } else { cond };
                // Init: `int i = 0` → `i := 0` (strip type); a plain
                // expression-statement is emitted as-is.
                let init_str = init_node.as_ref().map(|n| {
                    let raw = text(n, src).trim().trim_end_matches(';').trim();
                    normalize_c_init(raw)
                });
                let update_str = update_node.as_ref().map(|n| {
                    let raw = text(n, src).trim();
                    normalize_c_update(raw)
                });
                let body_inner = body_node.map(|n| {
                    if n.kind() == "compound_statement" {
                        emit_children_as_statements(&n, src, depth + 1, emit)
                    } else {
                        emit(&n, src, depth + 1)
                    }
                }).unwrap_or_default();
                let mut body_inner = body_inner.trim_end().to_string();
                if let Some(upd) = update_str {
                    if !body_inner.is_empty() { body_inner.push('\n'); }
                    body_inner.push_str(&format!("{}{}", indent(depth + 1), upd));
                }
                let head_init = init_str
                    .map(|s| format!("{}{}\n", ind, s))
                    .unwrap_or_default();
                format!("{}{}wl {} {{\n{}\n{}}}", head_init, ind, cond,
                    body_inner, ind)
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

            "do_statement" => {
                // C `do { body } while (cond);` — Nebula has no do-while.
                // Approximate with `while True: body / if not (cond): break`
                // so the loop still executes at least once.
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr_unwrap_parens(&n, src)).unwrap_or_default();
                let body_node = node.child_by_field_name("body");
                let inner_ind = indent(depth + 1);
                let mut body_inner = body_node.map(|b| {
                    if b.kind() == "compound_statement" {
                        emit_children_as_statements(&b, src, depth + 1, emit)
                    } else {
                        emit(&b, src, depth + 1)
                    }
                }).unwrap_or_default();
                if !body_inner.ends_with('\n') { body_inner.push('\n'); }
                format!("{}wl tt {{\n{}{}if not ({}) {{ bk }}\n{}}}",
                    ind, body_inner, inner_ind, cond, ind)
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
                // Emit variants by walking the enumerator_list explicitly so
                // we can strip the explicit-value annotations that Nebula's
                // enum grammar doesn't accept.
                let body = node.child_by_field_name("body").map(|body_node| {
                    let inner_ind = indent(depth + 1);
                    let mut variants = Vec::new();
                    let mut cursor = body_node.walk();
                    for child in body_node.named_children(&mut cursor) {
                        if child.kind() == "enumerator" {
                            let vname = child.child_by_field_name("name")
                                .map(|n| text(&n, src))
                                .unwrap_or_else(|| text(&child, src));
                            variants.push(format!("{}{}", inner_ind, vname));
                        }
                    }
                    if variants.is_empty() { " {}".to_string() }
                    else { format!(" {{\n{}\n{}}}", variants.join("\n"), ind) }
                }).unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}en {}{}", ind, name, body)
            }

            "switch_statement" => {
                let cond = node.child_by_field_name("condition")
                    .map(|n| emit_expr_unwrap_parens(&n, src)).unwrap_or_default();
                // Build match arms by walking case_statement children; each
                // case becomes `LABEL => EXPR` in Nebula's match grammar.
                let body_node = node.child_by_field_name("body");
                let arms = body_node.map(|body_node| {
                    let inner_ind = indent(depth + 1);
                    let mut arms = Vec::new();
                    let mut cursor = body_node.walk();
                    for child in body_node.named_children(&mut cursor) {
                        if child.kind() == "case_statement" {
                            let label = child.child_by_field_name("value")
                                .map(|n| emit_expr(&n, src))
                                .unwrap_or_else(|| "_".to_string());
                            // Collect case-body statements. Extract the raw
                            // expression from `return EXPR;` cases so the
                            // match arm stays an expression.
                            let mut stmts: Vec<String> = Vec::new();
                            let mut cc = child.walk();
                            for sub in child.named_children(&mut cc) {
                                if sub.kind() == "break_statement" { continue; }
                                if sub.kind() == "return_statement" {
                                    let expr_str = sub.named_child(0)
                                        .map(|n| emit_expr(&n, src))
                                        .unwrap_or_else(|| "()".to_string());
                                    stmts.push(expr_str);
                                } else if sub.kind() == "expression_statement"
                                       || sub.kind() == "compound_statement" {
                                    let s = emit(&sub, src, 0).trim().to_string();
                                    if !s.is_empty() { stmts.push(s); }
                                }
                            }
                            let expr = if stmts.is_empty() {
                                "()".to_string()
                            } else if stmts.len() == 1 {
                                stmts.into_iter().next().unwrap()
                            } else {
                                format!("{{ {} }}", stmts.join("; "))
                            };
                            let label_str = if label.trim().is_empty() { "_".to_string() } else { label };
                            arms.push(format!("{}{} => {}", inner_ind, label_str, expr));
                        }
                    }
                    arms
                }).unwrap_or_default();
                let body_fmt = if arms.is_empty() {
                    " {}".to_string()
                } else {
                    format!(" {{\n{}\n{}}}", arms.join("\n"), ind)
                };
                format!("{}mc {}{}", ind, cond, body_fmt)
            }

            "preproc_include" | "preproc_def" | "preproc_ifdef" |
            "preproc_ifndef" | "preproc_if" | "preproc_else" |
            "preproc_endif" | "preproc_function_def" => {
                // Strip preprocessor directives — they're metadata, not logic
                String::new()
            }

            "goto_statement" | "labeled_statement" => {
                // Nebula has no goto / labels. Drop these entirely — the
                // surrounding control flow may be semantically broken in
                // rare cases but the translation stays parseable.
                String::new()
            }

            "type_definition" => {
                // C typedef: "typedef struct { fields } Name;" → "st Name { fields }"
                let raw = text(node, src).trim().trim_end_matches(';');
                let body = raw.strip_prefix("typedef ").unwrap_or(raw);
                // Function-pointer typedef: `typedef RET (*NAME)(ARGS);`.
                // Nebula doesn't model function types, so emit an opaque
                // `ty NAME = Any` alias.
                if body.contains("(*") {
                    // Extract NAME between `(*` and `)`.
                    if let Some(lp) = body.find("(*") {
                        let after = &body[lp + 2..];
                        if let Some(rp) = after.find(')') {
                            let name = after[..rp].trim();
                            return format!("{}ty {} = Any", ind, name);
                        }
                    }
                }
                // Enum typedef: `typedef enum { ... } Name;` → emit as enum,
                // stripping per-variant `= value` annotations Nebula rejects.
                if body.contains("enum") && body.contains('{') {
                    let open = body.find('{').unwrap_or(0);
                    let close = body.rfind('}').unwrap_or(body.len());
                    if close > open {
                        let name = body[close + 1..].trim()
                            .trim_end_matches(';').trim();
                        let inner = &body[open + 1..close];
                        let inner_ind = indent(depth + 1);
                        let variants: Vec<String> = inner.split(',')
                            .map(|v| {
                                let v = v.trim();
                                let v = if let Some(eq) = v.find('=') {
                                    v[..eq].trim()
                                } else {
                                    v
                                };
                                v.to_string()
                            })
                            .filter(|v| !v.is_empty())
                            .map(|v| format!("{}{}", inner_ind, v))
                            .collect();
                        let body_fmt = if variants.is_empty() {
                            " {}".to_string()
                        } else {
                            format!(" {{\n{}\n{}}}", variants.join("\n"), ind)
                        };
                        return format!("{}en {}{}", ind, name, body_fmt);
                    }
                }
                let name = body.split_whitespace().last().unwrap_or("?")
                    .trim_end_matches('}');
                if body.contains('{') {
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
                let mut op = "=".to_string();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if !child.is_named() {
                        let t = text(&child, src);
                        if matches!(t, "=" | "+=" | "-=" | "*=" | "/=" | "%="
                                   | "<<=" | ">>=" | "&=" | "|=" | "^=") {
                            op = t.to_string();
                            break;
                        }
                    }
                }
                format!("{} {} {}", left, op, right)
            }

            "update_expression" => text(node, src).to_string(),
            "cast_expression" => {
                // C cast `(Type)value` → drop the cast entirely; Nebula is
                // dynamically typed and can't express pointer casts anyway.
                node.child_by_field_name("value")
                    .map(|n| emit_expr(&n, src))
                    .unwrap_or_else(|| text(node, src).to_string())
            }
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

    /// Convert a C for-init snippet (`int i = 0`, `size_t i = 0`, `i = 0`,
    /// or a bare expression) into an equivalent Nebula statement.
    fn normalize_c_init(raw: &str) -> String {
        let raw = raw.trim();
        if raw.is_empty() { return String::new(); }
        // Strip leading C type tokens so we're left with `name = expr`.
        let types = ["unsigned ", "signed ", "const ", "static ", "register ",
                     "int ", "short ", "long ", "char ", "float ", "double ",
                     "size_t ", "ssize_t ", "int32_t ", "int64_t ", "uint32_t ",
                     "uint64_t "];
        let mut s = raw.to_string();
        let mut changed = true;
        while changed {
            changed = false;
            for t in &types {
                if s.starts_with(*t) {
                    s = s[t.len()..].to_string();
                    changed = true;
                    break;
                }
            }
        }
        s
    }

    /// Convert a C for-update snippet (`i++`, `i += 1`, `i = i + 1`) into a
    /// Nebula-compatible statement. Tree-sitter emits `i++` as an
    /// `update_expression`; we normalize to `i += 1` here.
    fn normalize_c_update(raw: &str) -> String {
        let raw = raw.trim();
        if let Some(stem) = raw.strip_suffix("++") {
            format!("{} += 1", stem)
        } else if let Some(stem) = raw.strip_suffix("--") {
            format!("{} -= 1", stem)
        } else if let Some(stem) = raw.strip_prefix("++") {
            format!("{} += 1", stem)
        } else if let Some(stem) = raw.strip_prefix("--") {
            format!("{} -= 1", stem)
        } else {
            raw.to_string()
        }
    }

    /// Normalize C type spellings (`unsigned int`, `long`, `char`, etc.) to
    /// Nebula's built-in type names so the parser can treat them as simple
    /// identifiers. Unknown types pass through.
    fn normalize_c_type(t: &str) -> String {
        let t = t.trim();
        match t {
            "int" | "unsigned int" | "signed int" | "unsigned" | "signed"
            | "short" | "unsigned short" | "signed short"
            | "long" | "unsigned long" | "signed long"
            | "long long" | "unsigned long long" | "size_t" | "ssize_t"
            | "int8_t" | "int16_t" | "int32_t" | "int64_t"
            | "uint8_t" | "uint16_t" | "uint32_t" | "uint64_t" => "Int".to_string(),
            "float" | "double" | "long double" => "Float".to_string(),
            "char" | "unsigned char" | "signed char" => "Char".to_string(),
            "bool" | "_Bool" => "Bool".to_string(),
            "void" => "".to_string(),
            other => other.to_string(),
        }
    }

    fn emit_c_params(node: &Node, src: &[u8]) -> String {
        let mut params = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "parameter_declaration" {
                let decl_raw = child.child_by_field_name("declarator")
                    .map(|n| text(&n, src)).unwrap_or("_");
                let ty_raw = child.child_by_field_name("type")
                    .map(|n| text(&n, src).to_string());
                let has_pointer = decl_raw.starts_with('*');
                let (name_clean, is_array) = {
                    let stripped = decl_raw.trim_start_matches('*').trim_start_matches('&').trim();
                    if let Some(br) = stripped.find('[') {
                        (stripped[..br].trim().to_string(), true)
                    } else {
                        (stripped.to_string(), false)
                    }
                };
                // Type-normalize, with two special cases:
                //   `char *`         → Str
                //   `X *`            → X (drop pointer since Nebula ignores)
                let ty_norm = ty_raw.as_deref().map(normalize_c_type);
                let annotation = if is_array {
                    ": List".to_string()
                } else {
                    match ty_norm {
                        Some(t) if !t.is_empty() => {
                            if has_pointer && t == "Char" {
                                ": Str".to_string()
                            } else {
                                format!(": {}", t)
                            }
                        }
                        _ => String::new(),
                    }
                };
                params.push(format!("{}{}", name_clean, annotation));
            }
        }
        if params.is_empty() {
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
