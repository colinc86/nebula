/// Nebula compactor: compresses identifiers for LLM context efficiency.
///
/// `compact()` — shortens all user-defined identifiers and emits a @map header
/// `expand()` — reverses compaction using the @map header
use std::collections::HashMap;

/// Generate short identifiers: a, b, ..., z, aa, ab, ...
fn short_name(index: usize) -> String {
    let mut n = index;
    let mut name = String::new();
    loop {
        name.insert(0, (b'a' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    name
}

/// Reserved keywords that must NOT be shortened
const RESERVED: &[&str] = &[
    "lt", "mt", "cn", "fn", "rt", "if", "el", "mc", "fo", "wl",
    "lp", "bk", "ct", "st", "en", "tr", "im", "use", "md",
    "pb", "sf", "Sf", "ty", "ac", "aw", "sp", "try", "ch",
    "tt", "ff", "nil", "and", "or", "not", "in", "as",
    // Built-in functions (long form)
    "print", "println", "len", "type_of", "to_string", "to_int", "to_float",
    "assert", "assert_eq", "assert_ne", "abs", "sqrt", "floor", "ceil", "round",
    "min", "max", "PI", "E", "read_line", "range", "enumerate", "zip",
    // Built-in function aliases (short form)
    "prt", "ln", "tsr", "toi", "tof", "typ", "rng",
];

/// Compact Nebula source code: shorten identifiers and emit @map header.
pub fn compact(source: &str) -> String {
    // Collect all user-defined identifiers (non-reserved)
    let mut seen: Vec<String> = Vec::new();
    let mut in_string = false;
    let mut in_comment = false;
    let mut chars = source.chars().peekable();
    let mut current_ident = String::new();

    // Simple lexical scan for identifiers
    while let Some(&c) = chars.peek() {
        if in_comment {
            if c == '\n' {
                in_comment = false;
            }
            chars.next();
            continue;
        }

        if !in_string && c == '/' {
            chars.next();
            if chars.peek() == Some(&'/') {
                in_comment = true;
                chars.next();
                continue;
            }
            // Not a comment, push back conceptually
            if !current_ident.is_empty() {
                maybe_add_ident(&current_ident, &mut seen);
                current_ident.clear();
            }
            continue;
        }

        if c == '"' {
            in_string = !in_string;
            if !current_ident.is_empty() {
                maybe_add_ident(&current_ident, &mut seen);
                current_ident.clear();
            }
            chars.next();
            continue;
        }

        if in_string {
            chars.next();
            continue;
        }

        if c.is_ascii_alphanumeric() || c == '_' {
            current_ident.push(c);
            chars.next();
        } else {
            if !current_ident.is_empty() {
                maybe_add_ident(&current_ident, &mut seen);
                current_ident.clear();
            }
            chars.next();
        }
    }
    if !current_ident.is_empty() {
        maybe_add_ident(&current_ident, &mut seen);
    }

    if seen.is_empty() {
        return source.to_string();
    }

    // Sort by frequency of appearance (most common first = shortest names)
    let mut freq: HashMap<String, usize> = HashMap::new();
    for ident in &seen {
        *freq.entry(ident.clone()).or_insert(0) += count_occurrences(source, ident);
    }
    seen.sort_by(|a, b| freq.get(b).unwrap_or(&0).cmp(freq.get(a).unwrap_or(&0)));

    // Build the mapping
    // Skip short names that collide with existing identifiers or reserved words
    let mut mapping: HashMap<String, String> = HashMap::new();
    let mut name_idx = 0;
    for ident in &seen {
        // Don't shorten already-short identifiers (1-2 chars)
        if ident.len() <= 2 {
            continue;
        }
        loop {
            let short = short_name(name_idx);
            name_idx += 1;
            if !RESERVED.contains(&short.as_str())
                && !seen.contains(&short)
                && !mapping.values().any(|v: &String| *v == short)
            {
                mapping.insert(ident.clone(), short);
                break;
            }
        }
    }

    if mapping.is_empty() {
        return source.to_string();
    }

    // Build @map header
    let mut map_entries: Vec<String> = mapping
        .iter()
        .map(|(long, short)| format!("{short}:{long}"))
        .collect();
    map_entries.sort();

    // Also shorten built-in function names
    let builtin_aliases: Vec<(&str, &str)> = vec![
        ("print", "prt"), ("println", "prt"), ("to_string", "tsr"),
        ("to_int", "toi"), ("to_float", "tof"), ("type_of", "typ"),
        ("range", "rng"),
    ];
    for (long, short) in &builtin_aliases {
        if !mapping.contains_key(*long) {
            mapping.insert(long.to_string(), short.to_string());
        }
    }

    // Build @map header (only user-defined names, not builtin aliases)
    let mut map_entries: Vec<String> = mapping
        .iter()
        .filter(|(long, _)| !builtin_aliases.iter().any(|(b, _)| *b == long.as_str()))
        .map(|(long, short)| format!("{short}:{long}"))
        .collect();
    map_entries.sort();

    // Add @std header for builtin aliases used
    let mut std_entries: Vec<String> = builtin_aliases.iter()
        .filter(|(long, _)| count_occurrences(source, long) > 0)
        .map(|(long, short)| format!("{short}:{long}"))
        .collect();
    std_entries.sort();

    // Replace identifiers in source
    let compacted = replace_identifiers(source, &mapping);

    let mut header = String::new();
    if !map_entries.is_empty() {
        header.push_str(&format!("@map{{{}}}", map_entries.join(",")));
    }
    if !std_entries.is_empty() {
        if !header.is_empty() { header.push('\n'); }
        header.push_str(&format!("@std{{{}}}", std_entries.join(",")));
    }
    if header.is_empty() {
        compacted
    } else {
        format!("{header}\n{compacted}")
    }
}

/// Expand compacted source: read @map and @std headers, restore identifiers.
pub fn expand(source: &str) -> String {
    let mut mapping: HashMap<String, String> = HashMap::new();
    let mut body_start = 0;

    // Parse all header lines (@map{...} and @std{...})
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("@map{") && trimmed.ends_with('}') {
            let content = &trimmed[5..trimmed.len() - 1];
            for entry in content.split(',') {
                if let Some((short, long)) = entry.split_once(':') {
                    mapping.insert(short.to_string(), long.to_string());
                }
            }
            body_start += line.len() + 1; // +1 for newline
        } else if trimmed.starts_with("@std{") && trimmed.ends_with('}') {
            let content = &trimmed[5..trimmed.len() - 1];
            for entry in content.split(',') {
                if let Some((short, long)) = entry.split_once(':') {
                    mapping.insert(short.to_string(), long.to_string());
                }
            }
            body_start += line.len() + 1;
        } else {
            break;
        }
    }

    if mapping.is_empty() {
        return source.to_string();
    }

    let body = if body_start < source.len() { &source[body_start..] } else { "" };
    replace_identifiers(body, &mapping)
}

fn maybe_add_ident(ident: &str, seen: &mut Vec<String>) {
    // Skip numbers, reserved words, and already-seen
    if ident.chars().next().map_or(true, |c| c.is_ascii_digit()) {
        return;
    }
    if RESERVED.contains(&ident) {
        return;
    }
    if !seen.contains(&ident.to_string()) {
        seen.push(ident.to_string());
    }
}

fn count_occurrences(source: &str, ident: &str) -> usize {
    let mut count = 0;
    let bytes = source.as_bytes();
    let ident_bytes = ident.as_bytes();
    let len = ident_bytes.len();

    for i in 0..bytes.len() {
        if i + len > bytes.len() {
            break;
        }
        if &bytes[i..i + len] == ident_bytes {
            // Check word boundaries
            let before_ok =
                i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            let after_ok = i + len >= bytes.len()
                || !bytes[i + len].is_ascii_alphanumeric() && bytes[i + len] != b'_';
            if before_ok && after_ok {
                count += 1;
            }
        }
    }
    count
}

/// Replace identifiers in source text respecting word boundaries and strings.
fn replace_identifiers(source: &str, mapping: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    let mut in_line_comment = false;

    while i < chars.len() {
        if in_line_comment {
            result.push(chars[i]);
            if chars[i] == '\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }

        // Check for line comment
        if !in_string && chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            in_line_comment = true;
            result.push('/');
            result.push('/');
            i += 2;
            continue;
        }

        if chars[i] == '"' {
            in_string = !in_string;
            result.push('"');
            i += 1;
            continue;
        }

        if in_string {
            // Inside strings, still replace inside {} interpolation
            if chars[i] == '{' {
                result.push('{');
                i += 1;
                let mut depth = 1;
                while i < chars.len() && depth > 0 {
                    if chars[i] == '{' {
                        depth += 1;
                    } else if chars[i] == '}' {
                        depth -= 1;
                        if depth == 0 {
                            result.push('}');
                            i += 1;
                            break;
                        }
                    }

                    // Try to match identifiers inside interpolation
                    if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
                        let start = i;
                        while i < chars.len()
                            && (chars[i].is_ascii_alphanumeric() || chars[i] == '_')
                        {
                            i += 1;
                        }
                        let word: String = chars[start..i].iter().collect();
                        if let Some(replacement) = mapping.get(&word) {
                            result.push_str(replacement);
                        } else {
                            result.push_str(&word);
                        }
                    } else {
                        result.push(chars[i]);
                        i += 1;
                    }
                }
                continue;
            }

            result.push(chars[i]);
            i += 1;
            continue;
        }

        // Outside strings: try to match identifier
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if let Some(replacement) = mapping.get(&word) {
                result.push_str(replacement);
            } else {
                result.push_str(&word);
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_name_generation() {
        assert_eq!(short_name(0), "a");
        assert_eq!(short_name(25), "z");
        assert_eq!(short_name(26), "aa");
        assert_eq!(short_name(27), "ab");
    }

    #[test]
    fn test_compact_basic() {
        let source = "fn calculateTotal(amount, taxRate) = amount * taxRate";
        let compacted = compact(source);
        assert!(compacted.starts_with("@map{"));
        assert!(compacted.contains("calculateTotal"));
        // Should be shorter than original
        assert!(!compacted.contains("fn calculateTotal(amount, taxRate)"));
    }

    #[test]
    fn test_compact_preserves_keywords() {
        let source = "lt myVar = 42\nif myVar > 0 { prt(myVar) }";
        let compacted = compact(source);
        // Keywords must not be shortened
        assert!(compacted.contains("lt "));
        assert!(compacted.contains("if "));
        assert!(compacted.contains("prt("));
    }

    #[test]
    fn test_expand_reverses_compact() {
        let source = "fn calculateTotal(amount, taxRate) = amount + taxRate";
        let compacted = compact(source);
        let expanded = expand(&compacted);
        assert_eq!(expanded.trim(), source.trim());
    }

    #[test]
    fn test_roundtrip_preserves_semantics() {
        let source = "fn greet(userName) {\n    lt message = \"Hello, {userName}!\"\n    print(message)\n}";
        let compacted = compact(source);
        let expanded = expand(&compacted);
        assert_eq!(expanded.trim(), source.trim());
    }

    #[test]
    fn test_compact_does_not_shorten_short_names() {
        let source = "lt x = 1\nlt y = 2\nprint(x + y)";
        let compacted = compact(source);
        // x and y are only 1 char, should not be in @map
        assert!(!compacted.contains("@map{") || compacted.starts_with("@map{}"));
    }

    #[test]
    fn test_expand_no_header() {
        let source = "let x = 42";
        assert_eq!(expand(source), source);
    }

    #[test]
    fn test_compact_string_interpolation() {
        let source = "lt userName = \"Alice\"\nprint(\"Hi {userName}\")";
        let compacted = compact(source);
        let expanded = expand(&compacted);
        assert_eq!(expanded.trim(), source.trim());
    }
}
