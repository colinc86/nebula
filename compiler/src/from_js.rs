/// JavaScript/TypeScript → Nebula source-level translator.
///
/// Pattern-based conversion of common JS/TS syntax to Nebula.

pub fn convert(source: &str) -> String {
    let mut out = String::new();
    for line in source.lines() {
        out.push_str(&convert_line(line));
        out.push('\n');
    }
    out
}

fn convert_line(line: &str) -> String {
    let trimmed = line.trim();
    let indent_len = line.len() - line.trim_start().len();
    let indent: String = " ".repeat(indent_len);

    // Empty lines
    if trimmed.is_empty() {
        return String::new();
    }

    // Comments
    if trimmed.starts_with("//") {
        return line.to_string();
    }

    let mut s = trimmed.to_string();

    // Strip trailing semicolons
    if s.ends_with(';') && !s.ends_with(";;") {
        s = s[..s.len() - 1].to_string();
    }

    // const x = (args) => body → lt x = |args| body
    if s.starts_with("const ") && s.contains(" = ") && s.contains("=>") {
        let rest = &s[6..]; // strip "const "
        if let Some(eq_pos) = rest.find(" = ") {
            let name = &rest[..eq_pos];
            let value = convert_arrow(&rest[eq_pos + 3..]);
            return format!("{}lt {} = {}", indent, name, value);
        }
    }

    // const x = ... → lt x = ...
    if let Some(rest) = s.strip_prefix("const ") {
        return format!("{}lt {}", indent, convert_expr(rest));
    }

    // let x = (args) => body → mt x = |args| body
    if s.starts_with("let ") && s.contains(" = ") && s.contains("=>") {
        let rest = &s[4..];
        if let Some(eq_pos) = rest.find(" = ") {
            let name = &rest[..eq_pos];
            let value = convert_arrow(&rest[eq_pos + 3..]);
            return format!("{}mt {} = {}", indent, name, value);
        }
    }

    // let x = ... → mt x = ...
    if let Some(rest) = s.strip_prefix("let ") {
        return format!("{}mt {}", indent, convert_expr(rest));
    }

    // var x = ... → mt x = ...
    if let Some(rest) = s.strip_prefix("var ") {
        return format!("{}mt {}", indent, convert_expr(rest));
    }

    // function foo(...) { → fn foo(...) {
    if let Some(rest) = s.strip_prefix("function ") {
        return format!("{}fn {}", indent, convert_expr(rest));
    }

    // return expr → rt expr
    if let Some(rest) = s.strip_prefix("return ") {
        return format!("{}rt {}", indent, convert_expr(rest));
    }
    if s == "return" {
        return format!("{}rt", indent);
    }

    // console.log(...) → prt(...)
    if s.starts_with("console.log(") {
        let inner = &s[12..s.len() - 1]; // strip console.log( and )
        return format!("{}prt({})", indent, convert_expr(inner));
    }

    // if (...) { → if ... {
    if s.starts_with("if (") {
        let rest = &s[4..];
        // Find matching paren
        if let Some(paren_end) = find_matching_paren(rest) {
            let cond = &rest[..paren_end];
            let after = rest[paren_end + 1..].trim();
            return format!("{}if {} {}", indent, convert_expr(cond), after);
        }
    }

    // } else if (...) { → } el if ... {
    if s.starts_with("} else if (") {
        let rest = &s[11..];
        if let Some(paren_end) = find_matching_paren(rest) {
            let cond = &rest[..paren_end];
            let after = rest[paren_end + 1..].trim();
            return format!("{}}} el if {} {}", indent, convert_expr(cond), after);
        }
    }

    // } else { → } el {
    if s == "} else {" {
        return format!("{}}} el {{", indent);
    }

    // for (const x of arr) { → fo x in arr {
    if s.starts_with("for (") {
        if s.contains(" of ") {
            let inner = &s[5..s.rfind(')').unwrap_or(s.len())];
            let parts: Vec<&str> = inner.splitn(2, " of ").collect();
            if parts.len() == 2 {
                let binding = parts[0].replace("const ", "").replace("let ", "");
                return format!("{}fo {} in {} {{", indent, binding.trim(), convert_expr(parts[1]));
            }
        }
    }

    // while (...) { → wl ... {
    if s.starts_with("while (") {
        let rest = &s[7..];
        if let Some(paren_end) = find_matching_paren(rest) {
            let cond = &rest[..paren_end];
            let after = rest[paren_end + 1..].trim();
            return format!("{}wl {} {}", indent, convert_expr(cond), after);
        }
    }

    // break/continue
    if s == "break" { return format!("{}bk", indent); }
    if s == "continue" { return format!("{}ct", indent); }

    // class Foo { → st Foo {
    if let Some(rest) = s.strip_prefix("class ") {
        return format!("{}st {}", indent, rest);
    }

    // General
    format!("{}{}", indent, convert_expr(&s))
}

fn convert_expr(expr: &str) -> String {
    let mut e = expr.to_string();

    // JS → Nebula
    e = e.replace("true", "tt");
    e = e.replace("false", "ff");
    e = e.replace("null", "nil");
    e = e.replace("undefined", "nil");
    e = e.replace("===", "==");
    e = e.replace("!==", "!=");
    e = e.replace("console.log(", "prt(");
    e = e.replace(".length", ".ln()");
    e = e.replace(".push(", ".psh(");
    e = e.replace(".includes(", ".con(");
    e = e.replace(".forEach(", ".each(");
    e = e.replace("parseInt(", "toi(");
    e = e.replace("parseFloat(", "tof(");
    e = e.replace("String(", "tsr(");

    // Template literals: `text ${expr}` → "text {expr}"
    if e.contains('`') {
        e = e.replace('`', "\"");
        e = e.replace("${", "{");
    }

    // Arrow in expressions: (x) => expr → |x| expr
    if e.contains("=>") {
        e = convert_arrow(&e);
    }

    e
}

fn convert_arrow(s: &str) -> String {
    // Find => that's not inside parens (top-level arrow)
    let mut depth = 0;
    let chars: Vec<char> = s.chars().collect();
    let mut arrow_pos = None;
    for i in 0..chars.len().saturating_sub(1) {
        match chars[i] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '=' if depth == 0 && chars.get(i + 1) == Some(&'>') => {
                arrow_pos = Some(i);
                break;
            }
            _ => {}
        }
    }

    if let Some(pos) = arrow_pos {
        let before = s[..pos].trim();
        let after = s[pos + 2..].trim();

        let params = if before.starts_with('(') && before.ends_with(')') {
            before[1..before.len()-1]
                .split(',')
                .map(|p| p.split(':').next().unwrap_or(p).trim())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            before.split(':').next().unwrap_or(before).trim().to_string()
        };

        return format!("|{}| {}", params, convert_expr(after));
    }
    s.to_string()
}

fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1;
    for (i, c) in s.chars().enumerate() {
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
