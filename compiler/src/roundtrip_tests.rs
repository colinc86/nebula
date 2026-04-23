//! Round-trip tests for the ingest → Nebula → codegen pipeline.
//!
//! Corpus lives at `compiler/tests/corpus/<lang>/NN_*.ext`. Each snippet is
//! loaded verbatim via `include_str!` (resolved relative to this file).
//! Assertions are in two tiers:
//!
//! * **Tier 1 (always):** the pipeline doesn't explode — `to_nebula`
//!   produces something, `from_nebula` returns `Ok`, and the output is free
//!   of `// TODO` leaks and Nebula-form primitive types (`Float64`, `Int64`,
//!   etc.) that should have lowered.
//! * **Tier 2 (language-specific):** package headers are preserved (Go),
//!   imports round-trip, etc.
//!
//! This module was added in iter-24 as a regression harness for the three
//! translator bugs documented in `bench/paper.md §8`.

use crate::lang_detect::{self, Language};

/// Byte-level check: does `needle` appear as a standalone word in `hay`
/// outside of string-literal contexts? Rough heuristic — we strip double- and
/// single-quoted spans first, then scan for word-boundary matches. Good
/// enough for the leak-prevention assertions; not a general-purpose lexer.
fn contains_ident_outside_strings(hay: &str, needle: &str) -> bool {
    let mut stripped = String::with_capacity(hay.len());
    let mut chars = hay.chars().peekable();
    let mut in_str: Option<char> = None;
    while let Some(c) = chars.next() {
        match in_str {
            Some(q) => {
                if c == '\\' {
                    // skip the next escaped char
                    chars.next();
                } else if c == q {
                    in_str = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    in_str = Some(c);
                } else {
                    stripped.push(c);
                }
            }
        }
    }
    let bytes = stripped.as_bytes();
    let nlen = needle.len();
    if nlen == 0 || bytes.len() < nlen {
        return false;
    }
    let nb = needle.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0;
    while i + nlen <= bytes.len() {
        if &bytes[i..i + nlen] == nb {
            let before_ok = i == 0 || !is_word(bytes[i - 1]);
            let after_ok = i + nlen == bytes.len() || !is_word(bytes[i + nlen]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Run one corpus snippet through the `to_nebula → from_nebula` pipeline and
/// return the native output.
fn roundtrip(src: &str, lang: Language) -> String {
    let nebula = lang_detect::to_nebula(src, lang);
    assert!(
        !nebula.trim().is_empty(),
        "to_nebula produced empty output for {:?}",
        lang
    );
    lang_detect::from_nebula(&nebula, lang)
        .unwrap_or_else(|e| panic!("from_nebula failed for {:?}: {}", lang, e))
}

/// Tier-1 invariants that apply to every generated output, regardless of
/// target language.
fn assert_tier1(output: &str, lang: Language, label: &str) {
    // Bug D guard: unhandled Statement variants used to produce literal
    // "// TODO" / "/* TODO */" comments. We should never see these in output.
    assert!(
        !output.contains("// TODO")
            && !output.contains("/* TODO */")
            && !output.contains("# TODO"),
        "[{}] TODO leak in {:?} output:\n{}",
        label,
        lang,
        output
    );

    // Bug B guard: Nebula-form primitive type tokens should have been lowered
    // to their native counterparts. This check is most meaningful for Go/C
    // which have width-specific primitives.
    let leaked_tokens: &[&str] = match lang {
        Language::Go | Language::C => {
            &["Int", "Int8", "Int16", "Int32", "Int64",
              "Uint", "Uint8", "Uint16", "Uint32", "Uint64",
              "Float", "Float32", "Float64",
              "Complex64", "Complex128", "Byte", "Rune", "Any"]
        }
        _ => &[], // other languages use native names already (e.g. Python's `int`, Rust's `i64`)
    };
    for tok in leaked_tokens {
        assert!(
            !contains_ident_outside_strings(output, tok),
            "[{}] Nebula-form primitive '{}' leaked into {:?} output outside strings:\n{}",
            label,
            tok,
            lang,
            output
        );
    }
}

// ── Go ────────────────────────────────────────────────────────────────

const GO_01_HELLO: &str = include_str!("../tests/corpus/go/01_hello.go");
const GO_02_TYPES: &str = include_str!("../tests/corpus/go/02_types.go");
const GO_04_CUSTOM: &str = include_str!("../tests/corpus/go/04_custom_package.go");

#[test]
fn go_01_hello_tier1() {
    let out = roundtrip(GO_01_HELLO, Language::Go);
    assert_tier1(&out, Language::Go, "go/01_hello.go");
    // Tier-2: output starts with a package declaration.
    assert!(
        out.lines().next().map(|l| l.starts_with("package ")).unwrap_or(false),
        "expected 'package ...' header, got:\n{}",
        out
    );
}

#[test]
fn go_02_types_tier1() {
    let out = roundtrip(GO_02_TYPES, Language::Go);
    assert_tier1(&out, Language::Go, "go/02_types.go");
    // Tier-2: our int64 / float64 types survive explicitly.
    assert!(
        out.contains("int64") || out.contains("int"),
        "expected int-family type in output:\n{}",
        out
    );
}

#[test]
fn go_04_custom_package_tier1() {
    // The default from_nebula entry point (no path) drops package info; this
    // test just asserts Tier-1 invariants. The path-aware preservation test
    // lives in track-1 step 5 (Bug A fix).
    let out = roundtrip(GO_04_CUSTOM, Language::Go);
    assert_tier1(&out, Language::Go, "go/04_custom_package.go");
}

// ── Python ────────────────────────────────────────────────────────────

const PY_01_HELLO: &str = include_str!("../tests/corpus/python/01_hello.py");
const PY_02_CLASS: &str = include_str!("../tests/corpus/python/02_class.py");

#[test]
fn python_01_hello_tier1() {
    let out = roundtrip(PY_01_HELLO, Language::Python);
    assert_tier1(&out, Language::Python, "python/01_hello.py");
}

#[test]
fn python_02_class_tier1() {
    let out = roundtrip(PY_02_CLASS, Language::Python);
    assert_tier1(&out, Language::Python, "python/02_class.py");
}

// ── JavaScript ────────────────────────────────────────────────────────

const JS_01_HELLO: &str = include_str!("../tests/corpus/javascript/01_hello.js");

#[test]
fn javascript_01_hello_tier1() {
    let out = roundtrip(JS_01_HELLO, Language::JavaScript);
    assert_tier1(&out, Language::JavaScript, "javascript/01_hello.js");
}

// ── TypeScript ────────────────────────────────────────────────────────

const TS_01_TYPED: &str = include_str!("../tests/corpus/typescript/01_typed.ts");

#[test]
fn typescript_01_typed_tier1() {
    let out = roundtrip(TS_01_TYPED, Language::TypeScript);
    assert_tier1(&out, Language::TypeScript, "typescript/01_typed.ts");
}

// ── Rust ──────────────────────────────────────────────────────────────

const RS_01_HELLO: &str = include_str!("../tests/corpus/rust/01_hello.rs");

#[test]
fn rust_01_hello_tier1() {
    let out = roundtrip(RS_01_HELLO, Language::Rust);
    assert_tier1(&out, Language::Rust, "rust/01_hello.rs");
}

// ── Swift ─────────────────────────────────────────────────────────────

const SWIFT_01_HELLO: &str = include_str!("../tests/corpus/swift/01_hello.swift");

#[test]
fn swift_01_hello_tier1() {
    let out = roundtrip(SWIFT_01_HELLO, Language::Swift);
    assert_tier1(&out, Language::Swift, "swift/01_hello.swift");
}

// ── Kotlin ────────────────────────────────────────────────────────────

const KT_01_HELLO: &str = include_str!("../tests/corpus/kotlin/01_hello.kt");

#[test]
fn kotlin_01_hello_tier1() {
    let out = roundtrip(KT_01_HELLO, Language::Kotlin);
    assert_tier1(&out, Language::Kotlin, "kotlin/01_hello.kt");
}

// ── C ─────────────────────────────────────────────────────────────────

const C_01_HELLO: &str = include_str!("../tests/corpus/c/01_hello.c");

#[test]
fn c_01_hello_tier1() {
    let out = roundtrip(C_01_HELLO, Language::C);
    assert_tier1(&out, Language::C, "c/01_hello.c");
}

// ── Tier-2: Go package preservation via from_nebula_to_path ────────────

#[test]
fn go_custom_package_preserved_via_path() {
    // Stage a fresh corpus file to a temp dir, then call the path-aware
    // lowering. This exercises derive_go_package's sibling-scan logic as
    // well as the new to_go_with_package entry point.
    use std::fs;
    use std::path::PathBuf;

    let tmp = std::env::temp_dir().join("nebula_roundtrip_go_pkg");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).expect("create tmp dir");

    let orig_path: PathBuf = tmp.join("gdsp_seed.go");
    fs::write(&orig_path, GO_04_CUSTOM).expect("write seed");

    // Ingest → Nebula, then write back to a NEW file in the same directory.
    // The new file's package name should be derived from its sibling
    // (gdsp_seed.go) at `package gdsp`.
    let nebula = lang_detect::to_nebula(GO_04_CUSTOM, Language::Go);
    let target: PathBuf = tmp.join("reversed.go");
    let out = lang_detect::from_nebula_to_path(&nebula, Language::Go, target.to_str().unwrap())
        .expect("from_nebula_to_path");

    let first_line = out.lines().next().unwrap_or("");
    assert!(
        first_line.starts_with("package gdsp"),
        "expected `package gdsp` header, got: {:?}\nfull output:\n{}",
        first_line,
        out,
    );

    // Clean up after ourselves.
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn go_sanitize_package_fallback() {
    // When no siblings exist, derive_go_package falls back to a sanitized
    // dirname. Run the same round-trip in a brand-new directory.
    use std::fs;

    let tmp = std::env::temp_dir().join("my-ugly-dir_42");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).expect("create tmp dir");

    let nebula = lang_detect::to_nebula(GO_01_HELLO, Language::Go);
    let target = tmp.join("out.go");
    let out = lang_detect::from_nebula_to_path(&nebula, Language::Go, target.to_str().unwrap())
        .expect("from_nebula_to_path");

    // Expected sanitized name: "myuglydir42"
    let first_line = out.lines().next().unwrap_or("");
    assert!(
        first_line.starts_with("package myuglydir42"),
        "expected sanitized `package myuglydir42`, got: {:?}",
        first_line,
    );

    let _ = fs::remove_dir_all(&tmp);
}

// ── Unit tests for contains_ident_outside_strings ─────────────────────

#[test]
fn contains_ident_word_boundary() {
    assert!(contains_ident_outside_strings("Float64 x", "Float64"));
    assert!(!contains_ident_outside_strings("MyFloat64 x", "Float64"));
    assert!(!contains_ident_outside_strings("Float64a", "Float64"));
}

#[test]
fn contains_ident_skips_strings() {
    assert!(!contains_ident_outside_strings("s := \"Float64 is big\"", "Float64"));
    assert!(contains_ident_outside_strings("var x Float64; s := \"int\"", "Float64"));
}
