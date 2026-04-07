/// Python → Nebula source-level translator.
///
/// Performs pattern-based conversion of common Python syntax to Nebula.
/// Not a full parser — handles the most common patterns.

pub fn convert(source: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut indent_stack: Vec<usize> = vec![0];
    let src_lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < src_lines.len() {
        let line = src_lines[i];
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();

        // Skip empty lines
        if trimmed.is_empty() {
            lines.push(String::new());
            i += 1;
            continue;
        }

        // Close blocks when indent decreases
        while indent_stack.last().copied().unwrap_or(0) > indent {
            indent_stack.pop();
            let close_indent = "    ".repeat(indent_stack.len().saturating_sub(1));
            lines.push(format!("{}}}", close_indent));
        }

        let depth = indent_stack.len();
        let neb_indent = "    ".repeat(if depth > 0 { depth - 1 } else { 0 });

        // Comments
        if trimmed.starts_with('#') {
            lines.push(format!("{}//{}", neb_indent, &trimmed[1..]));
            i += 1;
            continue;
        }

        let converted = convert_line(trimmed, &neb_indent, &src_lines, i);

        // Check if this line opens a block (next line has greater indent)
        let next_indent = src_lines[i + 1..].iter()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .unwrap_or(0);

        if converted.ends_with('{') && next_indent > indent {
            indent_stack.push(next_indent);
        }

        lines.push(converted);
        i += 1;
    }

    // Close remaining blocks
    while indent_stack.len() > 1 {
        indent_stack.pop();
        let close_indent = "    ".repeat(indent_stack.len().saturating_sub(1));
        lines.push(format!("{}}}", close_indent));
    }

    lines.join("\n") + "\n"
}

fn convert_line(line: &str, indent: &str, _lines: &[&str], _idx: usize) -> String {
    // def foo(args): → fn foo(args) {
    if let Some(rest) = line.strip_prefix("def ") {
        let rest = rest.trim_end_matches(':');
        return format!("{}fn {} {{", indent, convert_params(rest));
    }

    // class Foo: → st Foo {
    if let Some(rest) = line.strip_prefix("class ") {
        let name = rest.trim_end_matches(':').split('(').next().unwrap_or(rest).trim();
        return format!("{}st {} {{", indent, name);
    }

    // if condition: → if condition {
    if let Some(rest) = line.strip_prefix("if ") {
        let cond = rest.trim_end_matches(':');
        return format!("{}if {} {{", indent, convert_expr(cond));
    }

    // elif condition: → } el if condition {
    if let Some(rest) = line.strip_prefix("elif ") {
        let cond = rest.trim_end_matches(':');
        return format!("{}}} el if {} {{", indent, convert_expr(cond));
    }

    // else: → } el {
    if line == "else:" {
        return format!("{}}} el {{", indent);
    }

    // for x in iterable: → fo x in iterable {
    if let Some(rest) = line.strip_prefix("for ") {
        let rest = rest.trim_end_matches(':');
        return format!("{}fo {} {{", indent, convert_expr(rest));
    }

    // while condition: → wl condition {
    if let Some(rest) = line.strip_prefix("while ") {
        let cond = rest.trim_end_matches(':');
        return format!("{}wl {} {{", indent, convert_expr(cond));
    }

    // return expr → rt expr
    if let Some(rest) = line.strip_prefix("return ") {
        return format!("{}rt {}", indent, convert_expr(rest));
    }
    if line == "return" {
        return format!("{}rt", indent);
    }

    // break/continue
    if line == "break" {
        return format!("{}bk", indent);
    }
    if line == "continue" {
        return format!("{}ct", indent);
    }

    // pass → // pass
    if line == "pass" {
        return format!("{}// pass", indent);
    }

    // try: → try {
    if line == "try:" {
        return format!("{}try {{", indent);
    }

    // except ... as e: → } ch e {
    if line.starts_with("except") {
        let binding = if let Some(pos) = line.find(" as ") {
            line[pos + 4..].trim_end_matches(':').trim()
        } else {
            "e"
        };
        return format!("{}}} ch {} {{", indent, binding);
    }

    // Variable assignment: x = expr
    if let Some((name, val)) = try_parse_assignment(line) {
        let val = convert_expr(val.trim());
        // Detect first assignment (use lt, not perfect but useful)
        return format!("{}lt {} = {}", indent, name, val);
    }

    // General expression
    format!("{}{}", indent, convert_expr(line))
}

fn convert_params(s: &str) -> String {
    // Convert self to sf in parameter lists
    s.replace("self, ", "sf, ")
     .replace("self)", "sf)")
     .replace("(self)", "(sf)")
}

fn convert_expr(expr: &str) -> String {
    let mut e = expr.to_string();

    // Python → Nebula keyword/function mapping
    e = e.replace("True", "tt");
    e = e.replace("False", "ff");
    e = e.replace("None", "nil");
    e = e.replace("print(", "prt(");
    e = e.replace(".append(", ".psh(");
    // Only replace standalone function calls (preceded by space, (, = or start of line)
    for (py, neb) in &[("len(", "ln("), ("str(", "tsr("), ("float(", "tof("), ("type(", "typ(")] {
        // Replace at start
        if e.starts_with(py) {
            e = format!("{}{}", neb, &e[py.len()..]);
        }
        // Replace after non-alphanumeric
        let mut result = String::new();
        let chars: Vec<char> = e.chars().collect();
        let mut j = 0;
        while j < chars.len() {
            if j > 0 && !chars[j-1].is_alphanumeric() && chars[j-1] != '_' {
                let remaining: String = chars[j..].iter().collect();
                if remaining.starts_with(py) {
                    result.push_str(neb);
                    j += py.len();
                    continue;
                }
            }
            result.push(chars[j]);
            j += 1;
        }
        e = result;
    }
    // int( is tricky — too many false positives, skip for now
    e = e.replace(" not ", " not ");
    e = e.replace(" and ", " and ");
    e = e.replace(" or ", " or ");

    // f-strings → Nebula interpolation (basic)
    if e.contains("f\"") || e.contains("f'") {
        e = e.replace("f\"", "\"");
        e = e.replace("f'", "\"");
        // Python f-string {expr} is already Nebula interpolation syntax
    }

    // lambda x: expr → |x| expr
    if let Some(pos) = e.find("lambda ") {
        if let Some(colon) = e[pos..].find(": ") {
            let params = &e[pos + 7..pos + colon];
            let body = &e[pos + colon + 2..];
            let before = &e[..pos];
            e = format!("{}|{}| {}", before, params, body);
        }
    }

    e
}

fn try_parse_assignment(line: &str) -> Option<(&str, &str)> {
    // Simple pattern: identifier = expr (not ==, +=, etc.)
    if let Some(eq_pos) = line.find(" = ") {
        let name = line[..eq_pos].trim();
        let val = &line[eq_pos + 3..];
        // Check it's a simple identifier
        if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
            // Not a comparison
            if !val.starts_with('=') {
                return Some((name, val));
            }
        }
    }
    None
}
