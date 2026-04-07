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
    strip_semicolons(&nebula)
}

pub fn from_kotlin(source: &str) -> String {
    // Kotlin uses pattern-based conversion (tree-sitter-kotlin depends on
    // tree-sitter 0.20 which conflicts with 0.26 at link time)
    let nebula = kotlin::convert_kotlin_raw(source, 0);
    strip_semicolons(&nebula)
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
    strip_semicolons(&nebula)
}

/// Post-process Nebula output to catch patterns the tree-sitter emitters miss.
fn postprocess(source: &str) -> String {
    source
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
        // Python boilerplate
        .replace("if __name__ == \"__main__\"", "// main guard")
        // Python builtins not caught by tree-sitter
        .replace("list(map(", "mp(")
        .replace("list(filter(", "fl(")
        .replace("list(mp(", "mp(")
        .replace("list(fl(", "fl(")
}

/// Strip trailing semicolons from each line (Nebula uses newlines as terminators).
fn strip_semicolons(source: &str) -> String {
    source.lines()
        .map(|line| line.trim_end_matches(';'))
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
fn map_builtin(name: &str) -> &str {
    match name {
        "print" | "println" | "puts" | "fmt.Println" | "fmt.Printf" | "console.log"
            | "console.warn" | "console.error" | "printf" | "fprintf" => "prt",
        "len" | "strlen" => "ln",
        "str" => "tsr",
        "int" => "toi",
        "float" | "parseFloat" => "tof",
        "type" | "isinstance" | "typeof" => "typ",
        "range" => "rng",
        "enumerate" => "enu",
        "append" | "push" => "psh",
        "sorted" => "srt",
        "abs" => "abs",
        "min" => "min",
        "max" => "max",
        "snprintf" | "sprintf" | "fmt.Sprintf" | "String.format" => "fmt",
        _ => name,
    }
}

/// Map common method names to Nebula equivalents.
fn map_method(name: &str) -> &str {
    match name {
        "append" | "push" | "push_back" => "psh",
        "length" | "count" | "size" | "len" => "ln",
        "contains" | "includes" | "containsKey" | "has" => "ck",
        "to_string" | "toString" | "String" | "string" => "tsr",
        "join" => "jn",
        "split" => "spl",
        "trim" | "strip" => "trm",
        "map" => "mp",
        "filter" | "retain" => "fl",
        "reduce" | "fold" | "fold_left" => "fld",
        "forEach" | "for_each" => "ea",
        "keys" => "keys",
        "values" | "vals" => "vals",
        "pop" | "removeLast" => "pop",
        "insert" => "ins",
        "sort" | "sorted" | "sortedBy" | "sort_by" | "sort_by_key" => "srt",
        "reverse" | "reversed" | "rev" => "rev",
        "iter" | "iterator" => "it",
        "iter_mut" => "itm",
        "collect" => "col",
        "cloned" | "copied" => "cln",
        "unwrap" | "unwrap_or" => "uw",
        "find" | "first" => "fnd",
        "isEmpty" | "is_empty" => "emp",
        "removeAll" | "clear" => "clr",
        "toFixed" | "format" => "fmt",
        "enumerate" | "enumerated" => "enu",
        "sum" => "sum",
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
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}st {}{}", ind, name, body)
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
                // For now, emit raw since comprehensions are complex
                text(node, src).to_string()
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
                let body = node.child_by_field_name("body")
                    .map(|n| emit_block(&n, src, depth))
                    .unwrap_or_else(|| format!(" {{\n{}}}", ind));
                format!("{}st {}{}", ind, name, body)
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
                // C-style for(init; cond; update) → fo i in range or wl cond
                let raw = text(node, src).trim();
                // Try to detect simple counting pattern: for (let i = START; i OP END; i++)
                format!("{}{}", ind, raw)
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
                        pairs.push(text(&child, src).to_string());
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
            if child.kind() == "property_signature" || child.kind() == "method_signature" {
                out.push_str(&format!("{}{}\n", inner_ind, text(&child, src).trim()));
            }
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
                .replace("\\(", "{")
                .replace("$0", "it")
                .replace("$1", "it2");
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
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                out.push(String::new());
                continue;
            }

            let converted = trimmed
                .replace("val ", "lt ")
                .replace("var ", "mt ")
                .replace("fun ", "fn ")
                .replace("return ", "rt ")
                .replace("data class ", "st ")
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
                // Emit as type alias
                let raw = text(node, src).trim().trim_end_matches(';');
                format!("{}ty {}", ind, raw.strip_prefix("typedef ").unwrap_or(raw))
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
