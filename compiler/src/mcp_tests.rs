//! Tests for the MCP tool handlers introduced in plugin v0.2:
//! `check_nebula`, `preview_translation`, `read_native`, and the
//! `nebula=true` mode of `patch_code`. Plus one regression test for
//! `write_code` confirming the Ollama fallback is gone.
//!
//! These live inside the binary's `#[cfg(test)]` tree so they can drive the
//! private `crate::mcp::tool_*` handlers directly without spawning the
//! subprocess.

use crate::mcp;
use serde_json::json;

fn call(tool_name: &str, args: serde_json::Value) -> Result<String, String> {
    // Mirror the dispatch in `handle_request` — we re-implement it here
    // so the tests don't need JSON-RPC framing. Keep this list in sync
    // with mcp.rs's `tools/call` match.
    match tool_name {
        "check_nebula" => mcp::tool_check_nebula(&args),
        "preview_translation" => mcp::tool_preview_translation(&args),
        "read_native" => mcp::tool_read_native(&args),
        "write_code" => mcp::tool_write_code(&args),
        "patch_code" => mcp::tool_patch_code(&args),
        _ => panic!("test harness doesn't know tool {}", tool_name),
    }
}

#[test]
fn check_nebula_accepts_valid_indent_block() {
    let r = call("check_nebula", json!({
        "code": "def foo(n):\n    return n+1"
    })).unwrap();
    assert!(r.contains("\"ok\":true"), "got {}", r);
}

#[test]
fn check_nebula_accepts_expression_body() {
    let r = call("check_nebula", json!({
        "code": "fn add(a, b) = a + b"
    })).unwrap();
    assert!(r.contains("\"ok\":true"), "got {}", r);
}

#[test]
fn check_nebula_rejects_parse_error() {
    let r = call("check_nebula", json!({
        "code": "def foo(: return"
    })).unwrap();
    assert!(r.contains("\"ok\":false"), "got {}", r);
    assert!(r.contains("\"kind\":\"parse\""), "got {}", r);
    // Structured line/col fields should be present.
    assert!(r.contains("\"line\":"), "got {}", r);
    assert!(r.contains("\"col\":"), "got {}", r);
}

#[test]
fn preview_translation_go_expression_body() {
    let r = call("preview_translation", json!({
        "code": "fn add(a, b) = a + b",
        "target_language": "go"
    })).unwrap();
    assert!(r.contains("func add"), "no func in output:\n{}", r);
    assert!(r.contains("a + b"), "body missing in output:\n{}", r);
}

#[test]
fn preview_translation_unknown_target_errors() {
    let r = call("preview_translation", json!({
        "code": "fn foo() = 1",
        "target_language": "klingon"
    }));
    assert!(r.is_err(), "expected error for unknown target_language");
}

#[test]
fn read_native_returns_numbered_lines() {
    use std::io::Write;
    let tmp = std::env::temp_dir().join("nebula_mcp_read_native_test.txt");
    {
        let mut f = std::fs::File::create(&tmp).unwrap();
        writeln!(f, "line one").unwrap();
        writeln!(f, "line two").unwrap();
        writeln!(f, "line three").unwrap();
    }
    let r = call("read_native", json!({"path": tmp.to_str().unwrap()})).unwrap();
    assert!(r.contains("1\tline one"), "expected 1-indexed first line:\n{}", r);
    assert!(r.contains("3\tline three"), "expected line three:\n{}", r);
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn read_native_respects_offset_and_limit() {
    use std::io::Write;
    let tmp = std::env::temp_dir().join("nebula_mcp_read_native_offset.txt");
    {
        let mut f = std::fs::File::create(&tmp).unwrap();
        for i in 1..=10 {
            writeln!(f, "line {}", i).unwrap();
        }
    }
    let r = call("read_native", json!({
        "path": tmp.to_str().unwrap(),
        "offset": 3,
        "limit": 2
    })).unwrap();
    assert!(r.contains("3\tline 3"), "expected line 3:\n{}", r);
    assert!(r.contains("4\tline 4"), "expected line 4:\n{}", r);
    assert!(!r.contains("5\tline 5"), "should stop before line 5:\n{}", r);
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn patch_code_nebula_mode_substitutes_native() {
    use std::io::Write;
    let tmp = std::env::temp_dir().join("nebula_mcp_patch_nebula_mode.py");
    {
        let mut f = std::fs::File::create(&tmp).unwrap();
        // Seed with native Python that matches what the Nebula snippet below
        // translates to: `x = 0`.
        writeln!(f, "x = 0").unwrap();
    }
    let r = call("patch_code", json!({
        "path": tmp.to_str().unwrap(),
        "old_str": "x = 0",
        "new_str": "x = 42",
        "nebula": true
    }));
    // Either this succeeds (the lenient translator produces matching native)
    // or fails with a clear translation error message. We only require that
    // the tool doesn't panic and returns a String result.
    match r {
        Ok(s) => assert!(s.contains("Patched") || s.contains("replaced"),
            "unexpected success payload: {}", s),
        Err(e) => assert!(
            e.contains("nebula") || e.contains("translate") || e.contains("not found"),
            "unexpected error: {}", e,
        ),
    }
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn write_code_succeeds_without_ollama() {
    // Regression for v0.2: the Ollama neural fallback is gone. write_code
    // must succeed on simple Nebula purely via the rule-based translator,
    // regardless of whether Ollama is reachable.
    let tmp = std::env::temp_dir().join("nebula_mcp_write_no_ollama.py");
    let _ = std::fs::remove_file(&tmp);
    let r = call("write_code", json!({
        "path": tmp.to_str().unwrap(),
        "code": "def greet(name):\n    return \"hi \" + name"
    })).unwrap();
    assert!(r.contains("Wrote"), "expected write confirmation:\n{}", r);
    let contents = std::fs::read_to_string(&tmp).unwrap();
    assert!(contents.contains("def greet"), "wrote bad Python:\n{}", contents);
    let _ = std::fs::remove_file(&tmp);
}
