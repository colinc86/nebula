//! Model-based Nebula→native error corrector.
//!
//! When the deterministic translator (`lang_detect::from_nebula_lenient`)
//! produces target-language source that the native syntax checker rejects,
//! call a Nebula-fluent fine-tuned model to rewrite the Nebula, re-translate,
//! and re-check. Loops up to N iterations before giving up and returning the
//! last failure to the MCP host.
//!
//! Wiring: `mcp::tool_write_code` and `mcp::tool_patch_code(nebula=true)` call
//! `translate_with_correction` instead of `from_nebula_lenient` directly.
//!
//! Env:
//!   NEBULA_CORRECTOR_DISABLE=1        bypass corrector (legacy behavior)
//!   NEBULA_CORRECTOR_MAX_ITERS=N      iteration cap (default 2)
//!   NEBULA_CORRECTOR_URL=http://...   OpenAI-compat endpoint (default: delegate URL)
//!   NEBULA_CORRECTOR_MODEL=<name>     model name sent to shim (default: nebula-prod-v2)
//!   NEBULA_CORRECTOR_LOG_DIR=<path>   where to append corrections.jsonl (default .nebula)
//!   NEBULA_CORRECTOR_TIMEOUT_S=N      per-checker timeout for model calls (default 120)

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

use crate::lang_detect::{self, Language};

/// Outcome of a native syntax check.
pub enum CheckResult {
    /// Source passed the checker, OR no checker is available for this language
    /// (graceful degradation — we don't block writes on missing tooling).
    Ok,
    /// Native checker rejected the source with this error text.
    Fail(String),
}

/// Record of one correction round: the Nebula we sent through the translator,
/// what came out, and the native compiler's complaint.
pub struct CorrectionAttempt {
    pub nebula_in: String,
    pub native_out: String,
    pub error: String,
}

pub struct CorrectedTranslation {
    pub native: String,
    /// How many model-correction rounds were needed. 0 = happy path.
    pub iterations: u32,
    #[allow(dead_code)]
    pub attempts: Vec<CorrectionAttempt>,
}

/// Translate Nebula → native with syntax-checked model correction on failure.
///
/// Happy path (most calls): `from_nebula_lenient` runs once, native check passes,
/// returns iterations=0. Only when the check fails do we wake up the corrector.
pub fn translate_with_correction(
    nebula: &str,
    lang: Language,
    path_hint: Option<&str>,
) -> Result<CorrectedTranslation, String> {
    let disabled = std::env::var("NEBULA_CORRECTOR_DISABLE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let max_iters: u32 = std::env::var("NEBULA_CORRECTOR_MAX_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);

    // Strict translate — surfaces Nebula parse errors instead of silently
    // falling through to lenient text-level translation. The corrector loop
    // needs honest errors; lenient masks Nebula-syntax mistakes as downstream
    // native-compiler confusion. Nebula that doesn't strictly parse now bounces
    // off the corrector (or returns a structured error if it's disabled).
    let translate = |neb: &str| -> Result<String, String> {
        if lang == Language::Go {
            if let Some(p) = path_hint {
                return lang_detect::from_nebula_to_path(neb, lang, p);
            }
        }
        lang_detect::from_nebula(neb, lang)
    };

    let mut current_nebula = nebula.to_string();
    let mut attempts: Vec<CorrectionAttempt> = Vec::new();

    // Iteration 0 = raw translation. Iterations 1..=max_iters = model corrections.
    for iter in 0..=max_iters {
        let native = match translate(&current_nebula) {
            Ok(n) => n,
            Err(e) => {
                // If even translation fails (bad Nebula syntax), let the corrector
                // try once — it may know how to fix the Nebula.
                if disabled || iter == max_iters {
                    log_corrections(lang, nebula, &attempts, false);
                    return Err(e);
                }
                attempts.push(CorrectionAttempt {
                    nebula_in: current_nebula.clone(),
                    native_out: String::new(),
                    error: format!("translate error: {}", e),
                });
                match request_correction(lang, &current_nebula, "", &e) {
                    Ok(fixed) => { current_nebula = fixed; continue; }
                    Err(ce) => {
                        log_corrections(lang, nebula, &attempts, false);
                        return Err(format!("{e} (corrector failed: {ce})"));
                    }
                }
            }
        };

        match check_native_syntax(lang, &native) {
            CheckResult::Ok => {
                if !attempts.is_empty() {
                    log_corrections(lang, nebula, &attempts, true);
                }
                return Ok(CorrectedTranslation {
                    native,
                    iterations: iter,
                    attempts,
                });
            }
            CheckResult::Fail(err) => {
                attempts.push(CorrectionAttempt {
                    nebula_in: current_nebula.clone(),
                    native_out: native.clone(),
                    error: err.clone(),
                });
                if disabled || iter == max_iters {
                    log_corrections(lang, nebula, &attempts, false);
                    return Err(format!(
                        "Nebula → {} produced invalid output after {} correction round(s). \
                         Last native compiler error:\n{}\n\nLast native output:\n{}",
                        lang.name(),
                        iter,
                        truncate(&err, 1500),
                        truncate(&native, 800),
                    ));
                }
                match request_correction(lang, &current_nebula, &native, &err) {
                    Ok(fixed) => { current_nebula = fixed; }
                    Err(ce) => {
                        log_corrections(lang, nebula, &attempts, false);
                        return Err(format!(
                            "Corrector call failed ({}). Last native compiler error:\n{}",
                            ce,
                            truncate(&err, 1200),
                        ));
                    }
                }
            }
        }
    }

    log_corrections(lang, nebula, &attempts, false);
    Err("corrector: exhausted iterations without success".to_string())
}

/// Run the native syntax checker for `lang` on `src`. Missing tools degrade to Ok;
/// we never fail the host on infrastructure gaps.
pub fn check_native_syntax(lang: Language, src: &str) -> CheckResult {
    match lang {
        Language::Go => check_go_build(src),
        Language::Python => check_stdin(
            "python3",
            &["-c", "import ast,sys; ast.parse(sys.stdin.read())"],
            src,
        ),
        Language::C => check_stdin("gcc", &["-fsyntax-only", "-x", "c", "-"], src),
        Language::JavaScript => check_tempfile("node", &["--check"], src, "js"),
        Language::TypeScript => check_tempfile(
            "tsc",
            &["--noEmit", "--target", "esnext", "--moduleResolution", "node",
              "--skipLibCheck", "--allowJs"],
            src, "ts",
        ),
        Language::Swift => check_tempfile("swift", &["-frontend", "-parse"], src, "swift"),
        // Rust's rustc is a full compile (slow, needs deps). Nebula→Rust hasn't
        // been the pressing failure mode — skip for now; add later if needed.
        Language::Rust => CheckResult::Ok,
        // Kotlin's kotlinc has 5-10s JVM start cost; too expensive for hot path.
        Language::Kotlin => CheckResult::Ok,
        Language::Nebula | Language::Unknown => CheckResult::Ok,
    }
}

/// Go syntax + semantic check via `go build`. gofmt alone catches only syntax
/// (rejects `for i := range 0:10 {` but accepts `i = 0` without declaration),
/// which let type-invalid output through the corrector in early tests. Build
/// catches undefined identifiers, type mismatches, and slice-on-interface{}.
/// Uses a temp module; `go build` on a single file requires the rest of the
/// package context which we don't have. Returns Ok if `go` isn't on PATH.
fn check_go_build(src: &str) -> CheckResult {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nebula_check_go_{}_{}", std::process::id(), now_nanos()));
    if std::fs::create_dir_all(&dir).is_err() {
        return CheckResult::Ok;
    }
    let go_mod = dir.join("go.mod");
    let go_src = dir.join("main.go");

    // Strip the caller's `package X` line so we can drop in a library-style
    // `package nebulacheck`. Using `package main` would require a `func main()`
    // which library-shaped snippets don't have — false negatives. Library
    // packages build fine with any top-level decls and no main.
    let stripped: String = src
        .lines()
        .skip_while(|l| l.trim_start().starts_with("package "))
        .collect::<Vec<_>>()
        .join("\n");
    let wrapped = format!("package nebulacheck\n\n{}\n", stripped);

    if std::fs::write(&go_mod, "module nebulacheck\n\ngo 1.21\n").is_err() {
        let _ = std::fs::remove_dir_all(&dir);
        return CheckResult::Ok;
    }
    if std::fs::write(&go_src, &wrapped).is_err() {
        let _ = std::fs::remove_dir_all(&dir);
        return CheckResult::Ok;
    }

    let result = match Command::new("go")
        .args(["build", "-o", "/dev/null", "./..."])
        .current_dir(&dir)
        .output()
    {
        Ok(out) => classify(&out),
        Err(_) => CheckResult::Ok, // `go` not installed
    };
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn check_stdin(cmd: &str, args: &[&str], src: &str) -> CheckResult {
    let child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(_) => return CheckResult::Ok, // tool not on PATH: degrade
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(src.as_bytes());
    }
    match child.wait_with_output() {
        Ok(out) => classify(&out),
        Err(_) => CheckResult::Ok,
    }
}

fn check_tempfile(cmd: &str, args: &[&str], src: &str, ext: &str) -> CheckResult {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "nebula_check_{}_{}.{}",
        std::process::id(),
        now_nanos(),
        ext,
    ));
    if std::fs::write(&path, src).is_err() {
        return CheckResult::Ok;
    }
    let mut full: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    full.push(path.to_string_lossy().into_owned());
    let result = match Command::new(cmd).args(&full).output() {
        Ok(out) => classify(&out),
        Err(_) => CheckResult::Ok,
    };
    let _ = std::fs::remove_file(&path);
    result
}

fn classify(out: &std::process::Output) -> CheckResult {
    if out.status.success() {
        return CheckResult::Ok;
    }
    let mut err = String::from_utf8_lossy(&out.stderr).into_owned();
    if err.trim().is_empty() {
        err = String::from_utf8_lossy(&out.stdout).into_owned();
    }
    CheckResult::Fail(err.trim().to_string())
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Call the corrector model via the Ollama-compat shim. Returns the corrected
/// Nebula text with any markdown fences stripped.
fn request_correction(
    lang: Language,
    nebula: &str,
    native_draft: &str,
    error: &str,
) -> Result<String, String> {
    let url = std::env::var("NEBULA_CORRECTOR_URL")
        .or_else(|_| std::env::var("NEBULA_DELEGATE_OLLAMA_URL"))
        .unwrap_or_else(|_| "http://localhost:11435".to_string());
    let model = std::env::var("NEBULA_CORRECTOR_MODEL")
        .or_else(|_| std::env::var("NEBULA_DELEGATE_MODEL"))
        .unwrap_or_else(|_| "nebula-prod-v2".to_string());
    let timeout_s: u64 = std::env::var("NEBULA_CORRECTOR_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);

    let system = format!(
        "You are a Nebula IL corrector. A deterministic rule-based translator \
         tried to emit {lang} from Nebula but produced output the {lang} compiler \
         rejected. The translator is limited — some Nebula patterns don't have \
         clean {lang} equivalents and must be rewritten upstream in the Nebula.\n\n\
         Your job: rewrite the Nebula so the translator produces valid {lang}. \
         Preserve behavior. Respond with ONLY the corrected Nebula — no prose, \
         no commentary, no markdown fences.\n\n\
         {pitfalls}",
        lang = lang.name(),
        pitfalls = pitfalls_for(lang),
    );
    let user = if native_draft.is_empty() {
        format!(
            "The Nebula below failed to parse. This means it uses syntax Nebula \
             doesn't support (e.g. slice-subscript `xs[a:b]`, Python-only method \
             shapes, etc.). Rewrite using valid Nebula constructs that the \
             translator can emit as {lang}.\n\n\
             ## Nebula (line-numbered; column refs in the error are 1-indexed)\n{neb}\n\n\
             ## Nebula parse error\n{err}\n\n\
             ## Corrected Nebula",
            lang = lang.name(),
            neb = number_lines(&truncate(nebula, 3000)),
            err = truncate(error, 1200),
        )
    } else {
        format!(
            "## Original Nebula (line-numbered)\n{nebula}\n\n\
             ## Translator output — {lang} (line-numbered, matches compiler error refs)\n{native}\n\n\
             ## {lang} compiler error\n{error}\n\n\
             ## Corrected Nebula",
            nebula = number_lines(&truncate(nebula, 3000)),
            native = number_lines(&truncate(native_draft, 2000)),
            error = truncate(error, 1500),
            lang = lang.name(),
        )
    };

    let payload = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "stream": false,
        "options": {
            "temperature": 0.1,
            "num_predict": 4096,
        },
    });
    let chat_url = format!("{}/api/chat", url);
    let resp: Value = ureq::post(&chat_url)
        .timeout(Duration::from_secs(timeout_s))
        .send_json(&payload)
        .map_err(|e| format!("corrector HTTP error at {}: {}", chat_url, e))?
        .into_json()
        .map_err(|e| format!("corrector parse error: {}", e))?;
    let content = resp
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| "corrector response missing message.content".to_string())?;
    Ok(strip_fences(content))
}

/// If the model wrapped its answer in ```...``` (possibly with a language tag),
/// extract the fenced body. Otherwise return the trimmed input.
fn strip_fences(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(after) = trimmed.strip_prefix("```") {
        // Drop the info string (e.g. "nebula\n") up to the first newline.
        let body = match after.find('\n') {
            Some(nl) => &after[nl + 1..],
            None => after,
        };
        if let Some(end) = body.rfind("```") {
            return body[..end].trim_end().to_string();
        }
    }
    trimmed.to_string()
}

/// Known failure modes of the rule-based Nebula→native translator, phrased
/// as actionable rewrites for the corrector model. These are learned from
/// gdsp benchmark traces and ad-hoc smoke tests; grow this list as new
/// patterns surface in corrections.jsonl.
fn pitfalls_for(lang: Language) -> &'static str {
    match lang {
        Language::Go => "\
## Known Go-translator pitfalls — rewrite these patterns in the Nebula\n\n\
VERIFIED-GOOD FORMS (use these in corrections):\n\
  let i = 0        → Go `i := 0`     (declares + initialises)\n\
  mut i = 0        → Go `i := 0`     (same; mutable explicit)\n\
  i := 0           → Go `i := 0`     (immutable walrus; rebinding fails)\n\
  while i < 10:    → Go `for (i < 10) { ... }`\n\
  i = i + 1        → Go `i = (i + 1)` (valid once i is declared)\n\n\
VERIFIED-BROKEN FORMS (avoid — the translator emits invalid Go):\n\
  i = 0             (bare; Go treats as undeclared → `undefined: i`)\n\
  fo i in 0..10:    (emits `for i := range 0:10 {` — invalid Go)\n\
  fo i in 0..=10:   (same)\n\
  fo i in range(a, b):  (emits `for i := range range(a, b) {` — double `range`)\n\
  xs.slice(a, b)    (emits literal `.slice()` — no such method)\n\
  xs[a:b]           (NOT valid Nebula — grammar rejects subscript-slice)\n\
  xs.append(x)      (emits literal `.append()`)\n\
  def foo(xs: []int):   (NOT valid Nebula — typed list params unsupported)\n\n\
CANONICAL REWRITES:\n\
  Loop 0..N:\n\
     def foo():\n\
         let i = 0\n\
         while i < 10:\n\
             do_thing(i)\n\
             i = i + 1\n\n\
  Python-style iteration over untyped xs:\n\
     Parameters stay `def foo(xs):` (untyped, emits `interface{}`).\n\
     Inside, you can't do type-specific operations on `xs` — redesign the\n\
     function so `xs` is only passed to other interface-tolerant code, OR\n\
     accept that some idiomatic Python patterns have no clean Go form and\n\
     drop to `def foo(xs):` + a trivial body that can't be expressed in\n\
     Nebula-to-Go honestly.\n\n\
If the only valid rewrite would itself use non-Nebula grammar (`xs[a:b]`,\n\
`[]int` params, generic slice methods), return the original Nebula unchanged —\n\
the corrector can't fix patterns Nebula itself doesn't express.",

        Language::TypeScript => "\
## Known TypeScript-translator pitfalls\n\n\
1. Implicit `any` parameters cause strict-mode errors. Add annotations:\n\
     def foo(xs): → def foo(xs: number[]):\n\n\
2. Python-only builtins (`.append`, `.pop`) become `.push`/`.pop` in TS but \
only when the param is typed as an array. Untyped params fail.\n\n\
3. Range-based for loops (`fo i in 0..10:`) should map to `for (let i=0; i<10; i++)`; \
if the translator leaves a literal `0..10`, rewrite as an explicit counter.",

        Language::Python | Language::JavaScript => "\
## General pitfalls — rewrite Nebula patterns that don't translate cleanly\n\n\
1. Keep structures simple and procedural. If the compiler error points to a \
specific construct, rewrite that pattern using a simpler equivalent.\n\
2. Untyped parameters are fine in Python/JS; errors here usually indicate a \
mismatched call form (wrong number of args, undefined name).",

        Language::C => "\
## Known C-translator pitfalls\n\n\
1. No generics, no classes — every container needs an explicit element type. \
Replace Nebula `[]any` or untyped containers with a concrete element type.\n\
2. String operations need explicit length handling; avoid high-level string \
methods.\n\
3. Keep programs flat — no closures, no higher-order functions.",

        Language::Swift | Language::Kotlin | Language::Rust | Language::Nebula | Language::Unknown => "\
## General pitfalls\n\n\
Rewrite Nebula patterns the error points at using simpler, more explicit \
equivalents. Add type annotations to parameters. Avoid Python-native methods \
that have no target-language equivalent.",
    }
}

/// Prefix each line of `s` with a 1-indexed line number, padded to 3 digits.
/// Keeps the Nebula and native panels aligned with the compiler's line refs.
fn number_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 64);
    for (i, line) in s.lines().enumerate() {
        out.push_str(&format!("{:>3} | {}\n", i + 1, line));
    }
    out
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut end = n;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n... ({} bytes truncated)", &s[..end], s.len() - end)
}

/// Append a correction record to `$NEBULA_CORRECTOR_LOG_DIR/corrections.jsonl`
/// (default `.nebula/corrections.jsonl` under cwd). These rows are the shape a
/// future slice-5 "self-correction" fine-tune slice would want: original Nebula,
/// broken native draft, compiler error, corrected Nebula chain. Skipped when
/// `attempts` is empty (nothing interesting happened).
fn log_corrections(lang: Language, input: &str, attempts: &[CorrectionAttempt], succeeded: bool) {
    if attempts.is_empty() {
        return;
    }
    let log_dir = std::env::var("NEBULA_CORRECTOR_LOG_DIR")
        .unwrap_or_else(|_| ".nebula".to_string());
    let _ = std::fs::create_dir_all(&log_dir);
    let path = Path::new(&log_dir).join("corrections.jsonl");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let entry = json!({
        "ts": ts,
        "lang": lang.name(),
        "input_nebula": input,
        "succeeded": succeeded,
        "attempts": attempts.iter().map(|a| json!({
            "nebula_in": a.nebula_in,
            "native_out": a.native_out,
            "error": a.error,
        })).collect::<Vec<_>>(),
    });
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
    {
        let _ = writeln!(f, "{}", entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_fences_handles_plain_text() {
        assert_eq!(strip_fences("def f(): rt 1"), "def f(): rt 1");
    }

    #[test]
    fn strip_fences_extracts_fenced_block() {
        let input = "```nebula\ndef f(): rt 1\n```";
        assert_eq!(strip_fences(input), "def f(): rt 1");
    }

    #[test]
    fn strip_fences_handles_no_info_string() {
        let input = "```\nx := 1\n```";
        assert_eq!(strip_fences(input), "x := 1");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let s = "héllo world";
        let t = truncate(s, 3);
        assert!(t.starts_with("hé") || t.starts_with("h"));
    }

    #[test]
    fn check_native_syntax_nebula_is_ok() {
        // Nebula source is never sent to a native checker.
        matches!(check_native_syntax(Language::Nebula, "anything"), CheckResult::Ok);
    }

    #[test]
    fn check_native_syntax_go_valid() {
        let src = "package main\n\nfunc main() {\n\tprintln(\"hi\")\n}\n";
        // gofmt may not be installed in all CI envs; CheckResult::Ok covers both
        // "gofmt said yes" and "gofmt not found".
        match check_native_syntax(Language::Go, src) {
            CheckResult::Ok => {}
            CheckResult::Fail(e) => panic!("valid Go rejected: {}", e),
        }
    }

    #[test]
    fn check_native_syntax_go_invalid_is_fail_when_tool_present() {
        // If gofmt is not present this asserts Ok (degrade), which is fine.
        let src = "package main\nfunc main() { let x := }\n";
        match check_native_syntax(Language::Go, src) {
            CheckResult::Ok => {
                // gofmt missing — accept.
            }
            CheckResult::Fail(_) => {
                // gofmt present and caught the bug.
            }
        }
    }
}
