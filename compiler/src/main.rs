#[allow(dead_code)]
mod ast;
mod codegen;
mod compactor;
mod formatter;
mod ingest;
mod interpreter;
mod lang_detect;
mod lexer;
mod linter;
mod mcp;
mod mcp_delegate;
mod neural;
mod parser;

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        repl();
        return;
    }

    // Handle -e "code" for inline execution
    if args.len() >= 3 && (args[1] == "-e" || args[1] == "--eval") {
        let code = args[2..].join(" ");
        run(&code, "<eval>");
        return;
    }

    // Handle `translate` command separately (it has its own arg parsing)
    if args.len() >= 2 && args[1] == "translate" {
        translate_command(&args[2..]);
        return;
    }

    let (command, file_path) = if args.len() == 2 {
        let arg = args[1].as_str();
        match arg {
            "repl" => { repl(); return; }
            "mcp" => { mcp::serve(); return; }
            "--help" | "-h" | "help" => { print_cli_help(); return; }
            "--version" | "-v" | "version" => { println!("Nebula v0.2.0"); return; }
            "builtins" | "api" => { print_builtins(); return; }
            _ => ("run", arg),
        }
    } else {
        (args[1].as_str(), args[2].as_str())
    };

    let raw_source = match fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{file_path}': {e}");
            process::exit(1);
        }
    };

    // Auto-expand compacted files (except for compact/expand commands)
    let source = if (raw_source.starts_with("@map{") || raw_source.starts_with("@std{"))
        && command != "expand" && command != "compact" && command != "minify"
    {
        compactor::expand(&raw_source)
    } else {
        raw_source
    };

    match command {
        "run" => run(&source, file_path),
        "test" => run_tests(&source, file_path),
        "check" => check(&source, file_path),
        "lint" => lint(&source, file_path),
        "to-python" | "to-py" => translate_to(&source, file_path, "python"),
        "to-javascript" | "to-js" => translate_to(&source, file_path, "javascript"),
        "to-go" => translate_to(&source, file_path, "go"),
        "to-swift" => translate_to(&source, file_path, "swift"),
        "to-kotlin" | "to-kt" => translate_to(&source, file_path, "kotlin"),
        "to-typescript" | "to-ts" => translate_to(&source, file_path, "typescript"),
        "to-rust" | "to-rs" => translate_to(&source, file_path, "rust"),
        "to-c" => translate_to(&source, file_path, "c"),
        "from-python" | "from-py" => print!("{}", ingest::from_python(&source)),
        "from-javascript" | "from-js" => print!("{}", ingest::from_javascript(&source)),
        "from-typescript" | "from-ts" => print!("{}", ingest::from_typescript(&source)),
        "from-go" => print!("{}", ingest::from_go(&source)),
        "from-swift" => print!("{}", ingest::from_swift(&source)),
        "from-kotlin" | "from-kt" => print!("{}", ingest::from_kotlin(&source)),
        "from-rust" | "from-rs" => print!("{}", ingest::from_rust(&source)),
        "from-c" => print!("{}", ingest::from_c(&source)),
        "fmt" | "format" => format_file(&source),
        "compact" | "minify" => print!("{}", compactor::compact(&source)),
        "expand" => print!("{}", compactor::expand(&source)),
        "stats" => show_stats(&source, file_path),
        "tokens" | "tokencount" => show_token_count(&source, file_path),
        other => {
            eprintln!("Unknown command: {other}");
            eprintln!("Usage: nebula [run|check|lint|fmt|compact|expand|to-*|from-*|mcp|repl] [file]");
            process::exit(1);
        }
    }
}

/// Print an error with source context: shows the offending line and a caret.
fn print_error(source: &str, file_path: &str, line: usize, col: usize, message: &str) {
    eprintln!("\x1b[1;31merror\x1b[0m: {message}");
    eprintln!("  \x1b[1;34m-->\x1b[0m {file_path}:{line}:{col}");
    let lines: Vec<&str> = source.lines().collect();
    if line > 0 && line <= lines.len() {
        let src_line = lines[line - 1];
        let line_num = format!("{line}");
        let pad = " ".repeat(line_num.len());
        eprintln!("  {pad} \x1b[1;34m|\x1b[0m");
        eprintln!("  {line_num} \x1b[1;34m|\x1b[0m {src_line}");
        let caret_pad = " ".repeat(col.saturating_sub(1));
        eprintln!("  {pad} \x1b[1;34m|\x1b[0m {caret_pad}\x1b[1;31m^\x1b[0m");
    }
}

fn lex_or_exit(source: &str, file_path: &str) -> Vec<lexer::SpannedToken> {
    match lexer::tokenize(source) {
        Ok(t) => t,
        Err(e) => {
            print_error(source, file_path, e.line, e.col, &e.message);
            process::exit(1);
        }
    }
}

fn parse_or_exit(
    source: &str,
    file_path: &str,
    tokens: Vec<lexer::SpannedToken>,
) -> ast::Program {
    match parser::parse(tokens) {
        Ok(p) => p,
        Err(e) => {
            print_error(source, file_path, e.line, e.col, &e.message);
            process::exit(1);
        }
    }
}

fn run(source: &str, file_path: &str) {
    let tokens = lex_or_exit(source, file_path);
    let program = parse_or_exit(source, file_path, tokens);

    let mut interp = interpreter::Interpreter::new();
    interp.set_file(file_path);
    if let Err(e) = interp.execute(&program) {
        eprintln!("\x1b[1;31merror\x1b[0m: {}", e.message);
        eprintln!("  \x1b[1;34m-->\x1b[0m {file_path}");
        process::exit(1);
    }
}

fn run_tests(source: &str, file_path: &str) {
    let tokens = lex_or_exit(source, file_path);
    let program = parse_or_exit(source, file_path, tokens);

    // Execute all declarations (register functions, structs, etc.)
    // but skip top-level expressions that aren't declarations
    let mut interp = interpreter::Interpreter::new();
    interp.set_file(file_path);
    for stmt in &program.statements {
        // Skip bare expressions (like print calls) during test setup
        if matches!(stmt, ast::Statement::Expr(_)) { continue; }
        if let Err(e) = interp.execute(&ast::Program { statements: vec![stmt.clone()] }) {
            eprintln!("\x1b[1;31merror\x1b[0m: {}", e.message);
            process::exit(1);
        }
    }

    // Collect and run test functions
    let test_fns: Vec<&str> = program.statements.iter().filter_map(|s| {
        if let ast::Statement::FnDecl { name, is_test: true, .. } = s {
            Some(name.as_str())
        } else {
            None
        }
    }).collect();

    if test_fns.is_empty() {
        println!("{file_path}: no test functions found");
        return;
    }

    let mut passed = 0;
    let mut failed = 0;
    for name in &test_fns {
        // Call the test function
        let call_source = format!("{}()", name);
        let tokens = lexer::tokenize(&call_source).unwrap();
        let call_prog = parser::parse(tokens).unwrap();
        match interp.execute(&call_prog) {
            Ok(()) => {
                println!("  \x1b[32m✓\x1b[0m {name}");
                passed += 1;
            }
            Err(e) => {
                println!("  \x1b[31m✗\x1b[0m {name}: {}", e.message);
                failed += 1;
            }
        }
    }
    println!();
    if failed == 0 {
        println!("\x1b[32m{passed} test{} passed\x1b[0m", if passed == 1 { "" } else { "s" });
    } else {
        println!("\x1b[31m{failed} failed\x1b[0m, {passed} passed");
        process::exit(1);
    }
}

fn check(source: &str, file_path: &str) {
    let tokens = lex_or_exit(source, file_path);
    let _ = parse_or_exit(source, file_path, tokens);
    println!("{file_path}: OK");
}

fn lint(source: &str, file_path: &str) {
    let tokens = lex_or_exit(source, file_path);
    let program = parse_or_exit(source, file_path, tokens);

    let warnings = linter::lint(&program);
    if warnings.is_empty() {
        println!("{file_path}: no issues found");
    } else {
        for w in &warnings {
            eprintln!(
                "\x1b[1;33mwarning\x1b[0m[{}]: {}",
                w.rule, w.message
            );
            eprintln!("  \x1b[1;34m-->\x1b[0m {file_path}");
        }
        process::exit(1);
    }
}

fn format_file(source: &str) {
    let tokens = lex_or_exit(source, "<stdin>");
    let program = parse_or_exit(source, "<stdin>", tokens);
    print!("{}", formatter::format(&program));
}

fn translate_to(source: &str, file_path: &str, target: &str) {
    let tokens = lex_or_exit(source, file_path);
    let program = parse_or_exit(source, file_path, tokens);

    let output = match target {
        "python" => codegen::to_python(&program),
        "javascript" => codegen::to_javascript(&program),
        "go" => codegen::to_go(&program),
        "swift" => codegen::to_swift(&program),
        "kotlin" => codegen::to_kotlin(&program),
        "typescript" => codegen::to_typescript(&program),
        "rust" => codegen::to_rust_lang(&program),
        "c" => codegen::to_c(&program),
        _ => unreachable!(),
    };
    print!("{output}");
}

fn show_stats(source: &str, file_path: &str) {
    let tokens = lex_or_exit(source, file_path);
    let token_count = tokens.iter().filter(|t| !matches!(t.token, lexer::Token::Newline | lexer::Token::Eof)).count();
    let program = parse_or_exit(source, file_path, tokens);
    let warnings = linter::lint(&program);

    let lines = source.lines().count();
    let code_lines = source.lines().filter(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with("//")
    }).count();
    let bytes = source.len();
    let compacted = compactor::compact(source);
    let compact_bytes = compacted.len();

    let fn_count = program.statements.iter().filter(|s| matches!(s, ast::Statement::FnDecl { .. })).count();
    let struct_count = program.statements.iter().filter(|s| matches!(s, ast::Statement::StructDecl { .. })).count();
    let enum_count = program.statements.iter().filter(|s| matches!(s, ast::Statement::EnumDecl { .. })).count();

    // Generate all targets for comparison
    let targets = [
        ("Python", codegen::to_python(&program).len()),
        ("JavaScript", codegen::to_javascript(&program).len()),
        ("TypeScript", codegen::to_typescript(&program).len()),
        ("Go", codegen::to_go(&program).len()),
        ("Rust", codegen::to_rust_lang(&program).len()),
    ];

    println!("{file_path}:");
    println!("  Lines:      {lines} ({code_lines} code)");
    println!("  Bytes:      {bytes} (compacted: {compact_bytes}, {:.0}% of original)", compact_bytes as f64 / bytes as f64 * 100.0);
    println!("  Tokens:     {token_count}");
    println!("  Functions:  {fn_count}");
    println!("  Structs:    {struct_count}");
    println!("  Enums:      {enum_count}");
    println!("  Warnings:   {}", warnings.len());
    println!();
    println!("  Density comparison (compacted Nebula vs target):");
    for (name, target_bytes) in &targets {
        let saving = (1.0 - compact_bytes as f64 / *target_bytes as f64) * 100.0;
        let indicator = if saving > 0.0 { "smaller" } else { "larger" };
        println!("    vs {name:12} {target_bytes:>5} bytes  ({:.0}% {indicator})", saving.abs());
    }
    println!("    Nebula compact: {compact_bytes:>5} bytes");
}

fn estimate_llm_tokens(text: &str) -> usize {
    // Approximation of cl100k_base tokenization for code.
    // Code averages ~3.5 bytes per token. Short identifiers and keywords
    // are often single tokens. This is a reasonable estimate.
    let bytes = text.len();
    // Count whitespace-separated words as a lower bound
    let words = text.split_whitespace().count();
    // Blend byte estimate with word estimate
    let byte_est = (bytes as f64 / 3.5).ceil() as usize;
    // Use the average of both heuristics
    (byte_est + words) / 2
}

fn show_token_count(source: &str, file_path: &str) {
    let tokens = lex_or_exit(source, file_path);
    let program = parse_or_exit(source, file_path, tokens);

    let neb_tokens = estimate_llm_tokens(source);
    let compacted = compactor::compact(source);
    let compact_tokens = estimate_llm_tokens(&compacted);

    let py = codegen::to_python(&program);
    let js = codegen::to_javascript(&program);
    let ts = codegen::to_typescript(&program);

    let py_tokens = estimate_llm_tokens(&py);
    let js_tokens = estimate_llm_tokens(&js);
    let ts_tokens = estimate_llm_tokens(&ts);

    println!("LLM token estimate for {file_path}:");
    println!();
    println!("  {:16} {:>6} tokens  {:>6} bytes", "Nebula", neb_tokens, source.len());
    println!("  {:16} {:>6} tokens  {:>6} bytes  ({:.0}% of Nebula)", "Nebula compact", compact_tokens, compacted.len(), compact_tokens as f64 / neb_tokens as f64 * 100.0);
    println!();
    println!("  Equivalent in other languages:");
    println!("  {:16} {:>6} tokens  (Nebula saves {:.0}%)", "Python", py_tokens, (1.0 - neb_tokens as f64 / py_tokens as f64) * 100.0);
    println!("  {:16} {:>6} tokens  (Nebula saves {:.0}%)", "JavaScript", js_tokens, (1.0 - neb_tokens as f64 / js_tokens as f64) * 100.0);
    println!("  {:16} {:>6} tokens  (Nebula saves {:.0}%)", "TypeScript", ts_tokens, (1.0 - neb_tokens as f64 / ts_tokens as f64) * 100.0);
    println!("  {:16} {:>6} tokens  (compact saves {:.0}%)", "Neb compact", compact_tokens, (1.0 - compact_tokens as f64 / ts_tokens as f64) * 100.0);
}

fn translate_command(args: &[String]) {
    let mut from_lang: Option<String> = None;
    let mut to_lang: Option<String> = None;
    let mut file_path: Option<&str> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--from" => {
                i += 1;
                if i < args.len() {
                    from_lang = Some(args[i].to_lowercase());
                }
            }
            "--to" => {
                i += 1;
                if i < args.len() {
                    to_lang = Some(args[i].to_lowercase());
                }
            }
            _ => {
                file_path = Some(&args[i]);
            }
        }
        i += 1;
    }

    let file_path = match file_path {
        Some(p) => p,
        None => {
            eprintln!("Usage: nebula translate [--from LANG] --to LANG <file>");
            process::exit(1);
        }
    };

    let target = match &to_lang {
        Some(t) => t.clone(),
        None => {
            eprintln!("Error: --to LANG is required");
            eprintln!("Usage: nebula translate [--from LANG] --to LANG <file>");
            process::exit(1);
        }
    };

    let source = match fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{file_path}': {e}");
            process::exit(1);
        }
    };

    // Determine source language: explicit --from or detect from extension
    let src_lang = match &from_lang {
        Some(l) => lang_detect::detect(&format!("file.{}", lang_to_ext(l))),
        None => lang_detect::detect(file_path),
    };

    // Ingest to Nebula (if not already Nebula)
    let nebula = lang_detect::to_nebula(&source, src_lang);

    // Try neural translation first, fall back to rule-based codegen
    if neural::is_available() {
        match neural::generate(&nebula, &target, None) {
            Ok(output) => {
                print!("{output}");
                return;
            }
            Err(e) => {
                eprintln!("Neural translation failed ({}), falling back to rule-based codegen", e);
            }
        }
    }

    // Fall back to rule-based codegen
    let target_lang = lang_detect::detect(&format!("file.{}", lang_to_ext(&target)));
    match lang_detect::from_nebula(&nebula, target_lang) {
        Ok(output) => print!("{output}"),
        Err(e) => {
            eprintln!("Translation failed: {e}");
            process::exit(1);
        }
    }
}

fn lang_to_ext(lang: &str) -> &str {
    match lang {
        "python" | "py" => "py",
        "javascript" | "js" => "js",
        "typescript" | "ts" => "ts",
        "go" => "go",
        "swift" => "swift",
        "kotlin" | "kt" => "kt",
        "rust" | "rs" => "rs",
        "c" => "c",
        "nebula" | "neb" => "neb",
        _ => lang, // pass through; detect() will return Unknown
    }
}

fn print_builtins() {
    println!("Nebula Built-in Functions & Aliases");
    println!("===================================");
    println!();
    println!("FUNCTIONS       alias   description");
    println!("  print(...)    prt     Print values to stdout");
    println!("  len(x)        ln      Length of string/list/map");
    println!("  type_of(x)    typ     Type name as string");
    println!("  to_string(x)  tsr     Convert to string");
    println!("  to_int(x)     toi     Convert to integer");
    println!("  to_float(x)   tof     Convert to float");
    println!("  range(n)      rng     List of ints [0..n)");
    println!("  range(a, b)   rng     List of ints [a..b)");
    println!("  abs(x)                Absolute value");
    println!("  sqrt(x)               Square root");
    println!("  min(a, b)             Minimum");
    println!("  max(a, b)             Maximum");
    println!("  floor(x)              Floor to int");
    println!("  ceil(x)               Ceiling to int");
    println!("  round(x)              Round to int");
    println!("  assert(cond)          Assert truthy");
    println!("  assert_eq(a, b)       Assert equal");
    println!("  assert_ne(a, b)       Assert not equal");
    println!("  assert_contains(a,b)  Assert a contains b");
    println!("  read_line()           Read line from stdin");
    println!();
    println!("LIST METHODS    alias   description");
    println!("  .len()        .ln()   Length");
    println!("  .map(f)       .mp()   Transform each element");
    println!("  .filter(f)    .fl()   Keep matching elements");
    println!("  .fold(init,f) .fd()   Reduce with accumulator");
    println!("  .reduce(f)    .rd()   Reduce (no initial value)");
    println!("  .sort()       .srt()  Sort ascending");
    println!("  .reverse()    .rv()   Reverse order");
    println!("  .unique()     .unq()  Remove duplicates");
    println!("  .count(f)     .cnt()  Count matching elements");
    println!("  .join(sep)    .jn()   Join to string");
    println!("  .push(x)     .psh()  Append element (returns new list)");
    println!("  .contains(x) .con()  Check if element exists");
    println!("  .first()              First element or nil");
    println!("  .last()               Last element or nil");
    println!("  .take(n)              First n elements");
    println!("  .skip(n)              Skip first n elements");
    println!("  .sum()                Sum of numbers");
    println!("  .min()                Minimum element");
    println!("  .max()                Maximum element");
    println!("  .flatten()            Flatten one level");
    println!("  .flat_map(f)          Map then flatten");
    println!("  .each(f)              Iterate for side effects");
    println!("  .zip(other)           Pair elements");
    println!("  .enumerate()          Add indices");
    println!();
    println!("STRING METHODS  description");
    println!("  .len()        Length");
    println!("  .contains(s)  Check substring");
    println!("  .split(sep)   Split to list");
    println!("  .trim()       Strip whitespace");
    println!("  .to_upper()   Uppercase");
    println!("  .to_lower()   Lowercase");
    println!("  .replace(a,b) Replace occurrences");
    println!("  .starts_with(s)  Check prefix");
    println!("  .ends_with(s)    Check suffix");
    println!("  .index_of(s)  Find position (-1 if absent)");
    println!("  .substring(a,b) Extract range");
    println!("  .chars()      List of characters");
    println!("  .rev()        Reverse string");
    println!("  .repeat(n)    Repeat n times");
    println!("  .is_empty()   Check if empty");
    println!();
    println!("MAP METHODS     description");
    println!("  .keys()       List of keys");
    println!("  .values()     List of values");
    println!("  .entries()    List of (key, value) tuples");
    println!("  .contains_key(k) .ck()  Check if key exists");
    println!("  .get(k)       Get value or nil");
    println!("  .get(k, def)  Get value or default");
    println!("  .len()        Number of entries");
}

fn print_cli_help() {
    println!("Nebula v0.2.0 — LLM-friendly programming language");
    println!();
    println!("USAGE:");
    println!("  nebula [file.neb]              Run a .neb file");
    println!("  nebula <command> [file.neb]    Run a command on a file");
    println!("  nebula -e 'code'              Evaluate inline code");
    println!("  nebula                         Start the REPL");
    println!();
    println!("COMMANDS:");
    println!("  run <file>       Execute a Nebula program (default)");
    println!("  test <file>      Run #[test] functions");
    println!("  check <file>     Parse-check without running");
    println!("  lint <file>      Run static analysis");
    println!("  fmt <file>       Format to canonical style");
    println!("  compact <file>   Compress identifiers for LLM context");
    println!("  expand <file>    Restore from compressed form");
    println!("  stats <file>     Show program statistics");
    println!("  builtins         List all built-in functions & methods");
    println!("  repl             Interactive REPL");
    println!();
    println!("NEURAL TRANSLATION:");
    println!("  translate --to LANG <file>       Neural translate (Ollama), falls back to rule-based");
    println!("  translate --from LANG --to LANG <file>");
    println!();
    println!("TRANSLATORS:");
    println!("  to-py <file>     Translate to Python");
    println!("  to-js <file>     Translate to JavaScript");
    println!("  to-ts <file>     Translate to TypeScript");
    println!("  to-go <file>     Translate to Go");
    println!("  to-swift <file>  Translate to Swift");
    println!("  to-kt <file>    Translate to Kotlin");
    println!("  to-rs <file>     Translate to Rust");
    println!("  to-c <file>      Translate to C");
    println!("  from-py <file>   Convert Python to Nebula");
    println!("  from-js <file>   Convert JavaScript/TypeScript to Nebula");
    println!("  from-go <file>   Convert Go to Nebula");
    println!();
    println!("OPTIONS:");
    println!("  --help, -h       Show this help");
    println!("  --version, -v    Show version");
}

fn repl() {
    println!("Nebula v0.2.0 REPL");
    println!("Type expressions or statements. Use :quit to exit, :help for help.");
    println!();

    let stdin = io::stdin();
    let mut interp = interpreter::Interpreter::new();
    let mut multiline_buf = String::new();
    let mut brace_depth: i32 = 0;

    loop {
        // Prompt
        if multiline_buf.is_empty() {
            print!("neb> ");
        } else {
            print!("...> ");
        }
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Err(e) => {
                eprintln!("Read error: {e}");
                break;
            }
            _ => {}
        }

        let trimmed = line.trim();

        // REPL commands
        if multiline_buf.is_empty() {
            match trimmed {
                ":quit" | ":q" | ":exit" => break,
                ":help" | ":h" => {
                    print_repl_help();
                    continue;
                }
                ":clear" | ":reset" => {
                    interp = interpreter::Interpreter::new();
                    println!("State cleared.");
                    continue;
                }
                "" => continue,
                _ => {}
            }
            // :load <file>
            if trimmed.starts_with(":load ") || trimmed.starts_with(":l ") {
                let path = trimmed.splitn(2, ' ').nth(1).unwrap_or("").trim();
                match fs::read_to_string(path) {
                    Ok(src) => {
                        let src = if src.starts_with("@map{") { compactor::expand(&src) } else { src };
                        match lexer::tokenize(&src).and_then(|t| parser::parse(t).map_err(|e| lexer::LexError { message: e.message, line: e.line, col: e.col })) {
                            Ok(prog) => {
                                match interp.execute(&prog) {
                                    Ok(()) => println!("Loaded {path}"),
                                    Err(e) => eprintln!("Runtime error: {e}"),
                                }
                            }
                            Err(e) => eprintln!("Parse error: {e}"),
                        }
                    }
                    Err(e) => eprintln!("Cannot read '{path}': {e}"),
                }
                continue;
            }
            // :type <expr>
            if trimmed.starts_with(":type ") || trimmed.starts_with(":t ") {
                let expr_src = trimmed.splitn(2, ' ').nth(1).unwrap_or("");
                if let Ok(tokens) = lexer::tokenize(expr_src) {
                    if let Ok(prog) = parser::parse(tokens) {
                        match interp.execute_interactive(&prog) {
                            Ok(Some(val)) => println!("{}: {}", val, val.type_name()),
                            Ok(None) => println!("nil: Nil"),
                            Err(e) => eprintln!("Error: {e}"),
                        }
                        continue;
                    }
                }
                eprintln!("Cannot parse expression");
                continue;
            }
        }

        // Track brace depth for multi-line input
        for ch in line.chars() {
            match ch {
                '{' | '(' | '[' => brace_depth += 1,
                '}' | ')' | ']' => brace_depth -= 1,
                _ => {}
            }
        }

        multiline_buf.push_str(&line);

        // If braces are still open, continue reading
        if brace_depth > 0 {
            continue;
        }
        brace_depth = 0;

        let source = multiline_buf.trim().to_string();
        multiline_buf.clear();

        if source.is_empty() {
            continue;
        }

        // Try to tokenize + parse + execute
        let tokens = match lexer::tokenize(&source) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Error: {e}");
                continue;
            }
        };

        let program = match parser::parse(tokens) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Error: {e}");
                continue;
            }
        };

        // Execute and print the result of the last expression
        match interp.execute_interactive(&program) {
            Ok(Some(val)) => println!("{val}"),
            Ok(None) => {}
            Err(e) => eprintln!("Error: {e}"),
        }
    }

    println!("\nGoodbye!");
}

fn print_repl_help() {
    println!("Nebula REPL commands:");
    println!("  :help, :h         Show this help");
    println!("  :quit, :q         Exit the REPL");
    println!("  :clear, :reset    Reset all state");
    println!("  :load <file>      Load and execute a .neb file");
    println!("  :type <expr>      Show expression value and type");
    println!();
    println!("Enter any Nebula expression or statement.");
    println!("Multi-line input is supported — open braces continue to the next line.");
    println!();
    println!("Examples:");
    println!("  lt x = 42");
    println!("  x * 2");
    println!("  fn double(n) = n * 2");
    println!("  [1,2,3].mp(|x| x * 10)");
}
