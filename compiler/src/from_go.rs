/// Go → Nebula source-level translator.
///
/// Pattern-based conversion of common Go syntax to Nebula.

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
    let indent = "\t".repeat(indent_len / 4).to_string()
        + &" ".repeat(indent_len % 4);

    if trimmed.is_empty() { return String::new(); }

    // Comments
    if trimmed.starts_with("//") { return line.to_string(); }

    // Skip package/import
    if trimmed.starts_with("package ") { return format!("{}// {}", indent, trimmed); }
    if trimmed == "import (" || trimmed.starts_with("import \"") { return format!("{}// {}", indent, trimmed); }
    if trimmed.starts_with("\"") && trimmed.ends_with("\"") { return format!("{}// {}", indent, trimmed); }
    if trimmed == ")" && indent_len > 0 { return format!("{}// )", indent); }

    let mut s = trimmed.to_string();

    // func name(params) returnType { → fn name(params) -> returnType {
    if s.starts_with("func ") {
        let rest = &s[5..];
        // Method: func (r Receiver) name(...) → fn name(sf, ...) {
        if rest.starts_with("(") {
            if let Some(paren_end) = rest.find(") ") {
                let method_rest = &rest[paren_end + 2..];
                return format!("{}fn {}", indent, convert_func_sig(method_rest, true));
            }
        }
        return format!("{}fn {}", indent, convert_func_sig(rest, false));
    }

    // type Name struct { → st Name {
    if s.starts_with("type ") && s.contains(" struct {") {
        let name = s[5..].split_whitespace().next().unwrap_or("?");
        return format!("{}st {} {{", indent, name);
    }

    // type Name interface { → tr Name {
    if s.starts_with("type ") && s.contains(" interface {") {
        let name = s[5..].split_whitespace().next().unwrap_or("?");
        return format!("{}tr {} {{", indent, name);
    }

    // for ... { → fo/wl/lp (MUST be before := check)
    if s.starts_with("for ") || s == "for {" {
        let rest = s[3..].trim().trim_end_matches(" {").trim_end_matches('{').trim();
        if rest.is_empty() {
            return format!("{}lp {{", indent);
        }
        if rest.contains(":= range ") {
            let parts: Vec<&str> = rest.splitn(2, ":= range ").collect();
            if parts.len() == 2 {
                let bindings: Vec<&str> = parts[0].split(',').map(|s| s.trim()).collect();
                let binding = if bindings.len() == 2 && bindings[0] == "_" {
                    bindings[1].to_string()
                } else if bindings.len() == 2 {
                    format!("{}, {}", bindings[0], bindings[1])
                } else {
                    bindings[0].to_string()
                };
                return format!("{}fo {} in {} {{", indent, binding, convert_expr(parts[1].trim()));
            }
        }
        if rest.contains(';') {
            let parts: Vec<&str> = rest.splitn(3, ';').collect();
            if parts.len() == 3 {
                let cond = parts[1].trim();
                return format!("{}// C-style for\n{}wl {} {{", indent, indent, convert_expr(cond));
            }
        }
        return format!("{}wl {} {{", indent, convert_expr(rest));
    }

    // var x type = val → mt x = val
    if let Some(rest) = s.strip_prefix("var ") {
        return format!("{}mt {}", indent, convert_var(rest));
    }

    // x := val → lt x = val
    if s.contains(":=") {
        let parts: Vec<&str> = s.splitn(2, ":=").collect();
        if parts.len() == 2 {
            return format!("{}lt {} = {}", indent, parts[0].trim(), convert_expr(parts[1].trim()));
        }
    }

    // return expr → rt expr
    if let Some(rest) = s.strip_prefix("return ") {
        return format!("{}rt {}", indent, convert_expr(rest));
    }
    if s == "return" { return format!("{}rt", indent); }

    // if condition { → if condition {
    if s.starts_with("if ") {
        let rest = s[3..].trim_end_matches(" {").trim_end_matches('{');
        return format!("{}if {} {{", indent, convert_expr(rest.trim()));
    }

    // } else if { → } el if {
    if s.starts_with("} else if ") {
        let rest = s[10..].trim_end_matches(" {").trim_end_matches('{');
        return format!("{}}} el if {} {{", indent, convert_expr(rest.trim()));
    }

    // } else { → } el {
    if s == "} else {" { return format!("{}}} el {{", indent); }

    // break/continue
    if s == "break" { return format!("{}bk", indent); }
    if s == "continue" { return format!("{}ct", indent); }

    // fmt.Println(...) → prt(...)
    s = s.replace("fmt.Println(", "prt(");
    s = s.replace("fmt.Printf(", "prt(");
    s = s.replace("fmt.Sprintf(", "tsr(");

    format!("{}{}", indent, convert_expr(&s))
}

fn convert_func_sig(s: &str, has_receiver: bool) -> String {
    // name(params) retType {
    let self_prefix = if has_receiver { "sf, " } else { "" };

    if let Some(paren_start) = s.find('(') {
        let name = &s[..paren_start];
        let rest = &s[paren_start + 1..];
        if let Some(paren_end) = rest.find(')') {
            let params_raw = &rest[..paren_end];
            let after = rest[paren_end + 1..].trim().trim_end_matches('{').trim();

            // Convert params: strip Go types
            let params: Vec<String> = if params_raw.is_empty() {
                vec![]
            } else {
                params_raw.split(',').map(|p| {
                    let parts: Vec<&str> = p.trim().split_whitespace().collect();
                    parts[0].to_string() // just the name, drop the type
                }).collect()
            };

            let all_params = if has_receiver && !params.is_empty() {
                format!("{}{}", self_prefix, params.join(", "))
            } else if has_receiver {
                "sf".to_string()
            } else {
                params.join(", ")
            };

            let ret = if !after.is_empty() && after != "{" {
                format!(" -> {}", go_type_to_neb(after))
            } else {
                String::new()
            };

            return format!("{}({}){} {{", name, all_params, ret);
        }
    }
    format!("{} {{", s)
}

fn convert_var(s: &str) -> String {
    // x int = 5 → x = 5  OR  x = 0
    let parts: Vec<&str> = s.splitn(2, "=").collect();
    if parts.len() == 2 {
        let name_type = parts[0].trim();
        let val = parts[1].trim();
        let name = name_type.split_whitespace().next().unwrap_or(name_type);
        format!("{} = {}", name, convert_expr(val))
    } else {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() >= 2 {
            format!("{} = nil", parts[0])
        } else {
            format!("{} = nil", s.trim())
        }
    }
}

fn convert_expr(expr: &str) -> String {
    let mut e = expr.to_string();
    e = e.replace("true", "tt");
    e = e.replace("false", "ff");
    e = e.replace("nil", "nil");
    e = e.replace("fmt.Println(", "prt(");
    e = e.replace("fmt.Sprintf(", "tsr(");
    e = e.replace("len(", "ln(");
    e = e.replace("append(", "psh(");
    e
}

fn go_type_to_neb(t: &str) -> String {
    match t.trim() {
        "int" | "int64" | "int32" => "Int".into(),
        "float64" | "float32" => "Float".into(),
        "string" => "String".into(),
        "bool" => "Bool".into(),
        "error" => "String".into(),
        other => other.to_string(),
    }
}
