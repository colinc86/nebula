---
name: add-feature
description: Guide for adding a new language feature through the full compiler pipeline
user_invocable: true
---

# Add a New Language Feature

You are adding a new language feature to Nebula. Follow this pipeline in order — each step builds on the previous.

## Before you start
1. Read `spec/LANGUAGE.md` to check if this feature is already specified. If it is, follow the spec exactly. If not, design it to be **brief, consistent, and translatable** per Nebula's design principles, then update the spec after implementation.
2. Read the relevant sections of the existing code to understand current patterns.

## Pipeline (in order)

### 1. AST (`compiler/src/ast.rs`)
- Add new variant(s) to `Statement`, `Expr`, or create new supporting types
- Rust's exhaustive matching will then guide you to every place that needs updating

### 2. Lexer (`compiler/src/lexer.rs`) — if new tokens are needed
- Add token variant to the `Token` enum
- Add lexing logic in `tokenize()`
- Add lexer tests

### 3. Parser (`compiler/src/parser.rs`)
- Add parsing logic (this is a Pratt-style recursive descent parser)
- Handle precedence if it's an operator
- Add parser tests

### 4. Interpreter (`compiler/src/interpreter.rs`)
- Add execution logic in the relevant `match` arms
- Add interpreter tests covering normal cases, edge cases, and error cases

### 5. Codegen (`compiler/src/codegen.rs`) — ALL 5 targets
Update **every** code generator. They all follow the same pattern:
- `PythonGen` — Python target
- `JsGen` — JavaScript target
- `GoGen` — Go target
- `SwiftGen` — Swift target
- `KotlinGen` — Kotlin target

Each has `gen_statement()` and `gen_expr()` methods with match arms. Add the new arm to all 5. Add codegen tests for each target.

### 6. Formatter (`compiler/src/formatter.rs`)
- Add formatting logic so the feature round-trips correctly (parse → format → parse → format = identical)
- Add formatter tests

### 7. Linter (`compiler/src/linter.rs`) — if applicable
- Consider if any lint rules should apply to the new feature

### 8. Compactor (`compiler/src/compactor.rs`) — if applicable
- If identifiers are involved, ensure compaction/expansion handles them

## After implementation
1. Run `cargo test` — all tests must pass, zero warnings
2. Run all example files: `for f in examples/*.neb; do cargo run -- "$f"; done`
3. Update `spec/LANGUAGE.md` if the feature is new
4. Consider adding an example `.neb` file demonstrating the feature
