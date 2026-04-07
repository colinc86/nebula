---
name: spec-check
description: Audit compiler implementation against the language spec for compliance gaps
user_invocable: true
---

# Spec Compliance Audit

Compare the language specification against the actual compiler implementation and report gaps.

## Process

### 1. Read the spec
Read `spec/LANGUAGE.md` in full. This is the source of truth for what Nebula should support.

### 2. Read the implementation
Read these files to understand what's actually implemented:
- `compiler/src/ast.rs` — what AST nodes exist
- `compiler/src/lexer.rs` — what tokens are recognized
- `compiler/src/parser.rs` — what syntax is parsed
- `compiler/src/interpreter.rs` — what semantics are implemented
- `compiler/src/codegen.rs` — what translates to target languages

### 3. Cross-reference and report

Produce a table with three sections:

**Implemented and tested** — Features in both spec and code, with test coverage.

**Specified but not implemented** — Features described in the spec that the compiler doesn't handle yet. For each, note which pipeline stages are missing (lexer? parser? interpreter? codegen?).

**Implemented but not in spec** — Features that work in the compiler but aren't documented in the spec. These should be added to the spec or removed from the code.

### 4. Priority recommendations
Suggest which gaps to close first, considering:
- Features that users would expect to work (based on examples and spec prominence)
- Features that block other features
- Features that are partially implemented (cheapest to finish)

## Output format
Keep the report concise. Use tables and bullet points, not prose. Focus on actionable gaps.
