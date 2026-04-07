# Nebula

An LLM-friendly programming language designed for maximum code density and universal translatability.

## Project Structure

```
spec/LANGUAGE.md        Language specification v0.2 (source of truth)
compiler/               Rust compiler + interpreter + toolchain
  src/lexer.rs          Tokenizer
  src/parser.rs         Pratt-style recursive descent parser
  src/ast.rs            Abstract syntax tree types
  src/interpreter.rs    Tree-walk interpreter
  src/linter.rs         Static analysis (unused vars, unnecessary mut, unreachable code, shadowed vars, constant conditions)
  src/codegen.rs        Translators (Nebula → Python, JS, TS, Go, Swift, Kotlin, Rust, C)
  src/ingest.rs         Reverse translators (Python, JS, TS, Go, Swift, Kotlin, Rust, C → Nebula) via tree-sitter
  src/mcp.rs            MCP server (JSON-RPC over stdio) — read_code, write_code, search_code tools
  src/lang_detect.rs    File extension → language detection and dispatch
  src/formatter.rs      Code formatter (AST → canonical Nebula)
  src/main.rs           CLI entrypoint (run, check, lint, fmt, compact, expand, to-*, from-*, mcp, repl)
examples/               Working .neb example programs
```

## Commands

```sh
cd compiler
cargo build                              # Build
cargo test                               # Run all tests (255)
cargo run -- test examples/tests_demo.neb # Run #[test] functions
cargo run -- examples/hello.neb          # Run a .neb file
cargo run -- check examples/hello.neb    # Parse-check only
cargo run -- lint examples/hello.neb     # Run linter
cargo run -- fmt examples/hello.neb      # Format to canonical style
cargo run -- compact examples/hello.neb  # Compress identifiers + @map header
cargo run -- expand compacted.neb        # Restore from compressed form
cargo run -- to-python examples/hello.neb # Translate to Python
cargo run -- to-js examples/hello.neb    # Translate to JavaScript
cargo run -- to-ts examples/hello.neb    # Translate to TypeScript
cargo run -- to-rs examples/hello.neb    # Translate to Rust
cargo run -- from-python app.py          # Ingest Python → Nebula
cargo run -- from-js app.js              # Ingest JavaScript → Nebula
cargo run -- from-rs lib.rs              # Ingest Rust → Nebula
cargo run -- mcp                         # Start MCP server (JSON-RPC over stdio)
cargo run                                # Start REPL
```

## Design Principles

1. **Brief** — Nebula is terse. Minimize tokens per concept. LLMs should fit more Nebula into context than equivalent code in other languages.
2. **Consistent** — No special cases. Every construct follows the same patterns.
3. **Translatable** — Any language maps cleanly to/from Nebula. The translator (`nebula translate`) converts between Nebula and other languages.
4. **Multi-paradigm** — FP, OOP, procedural all first-class.
5. **Explicit** — Code reads like intent. No hidden magic.

## Key Syntax

- `lt`/`mt` bindings, `fn` functions, `|x| expr` lambdas
- `and`/`or`/`not` logical ops, `|>` pipe, `??` nil coalesce
- `{}` string interpolation, `..`/`..=` ranges
- `tt`/`ff` for booleans, `sf` for self
- Keywords are maximally compressed: `st`(struct), `en`(enum), `im`(impl), `tr`(trait), `fo`(for), `wl`(while), `lp`(loop), `mc`(match), `rt`(return), `bk`(break), `ct`(continue), `el`(else)
- Newlines as statement terminators (no semicolons)

## When Working on This Project

- Run `cargo test` after every change (auto-runs via PostToolUse hook)
- The spec at `spec/LANGUAGE.md` is the design authority
- Keep Nebula syntax **brief** — every token must earn its place
- Zero compiler warnings policy — build must be clean
- All codegen targets in `codegen.rs` follow the same `XxxGen { indent }` struct pattern — copy `PythonGen` when adding new targets
- AST enums use exhaustive matching — adding a new variant will cause compiler errors at every match that needs updating; follow the errors
- All source files are small enough to read in full (~16K lines total across 12 modules)
