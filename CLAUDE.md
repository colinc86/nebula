# Nebula

A universal code intermediate language for token-efficient LLM code generation. Nebula normalizes code from 8 programming languages into a single canonical form, compressing 3-21% fewer tokens than the source. Extends SimPy (ISSTA 2024, arXiv:2404.16333) and Token Sugar (ASE 2025, arXiv:2512.08266) to multiple languages.

## Project Direction (as of 2026-04)

Nebula 1.0 is a **universal code IL** — one canonical form for any programming language. All 6 phases complete. Experiment log: `bench/experiments.md`. Results: `bench/results.md`.

**Completed:**
- Phase 1: Canonical normalization (all 8 ingesters converge)
- Phase 2: Nebula 1.0 syntax (walrus `:=`, indent blocks, Python-length keywords, `class` sugar)
- Phase 3: Cross-language compression benchmark (16.2% overall, all 8 languages compress)
- Phase 4: InterTrans validation — Nebula-mediated Python→Go +7.5pp over direct (75.0% vs 67.5% CA)
- Phase 5: Neural refinement — fine-tuned Qwen-2.5-1.5B achieves 48.8% Pass@1 on Nebula→Python (3-epoch, vs 1.2% base)
- Phase 6: Error correction pipeline — retry loop + rule-based fallback reaches 78.7% Pass@1

## Project Structure

```
spec/LANGUAGE.md        Language specification (source of truth)
compiler/               Rust compiler + interpreter + toolchain
  src/lexer.rs          Tokenizer (indent/dedent desugaring, dual keyword forms)
  src/parser.rs         Recursive descent parser (brace + indent blocks, walrus :=)
  src/ast.rs            Language-agnostic abstract syntax tree
  src/interpreter.rs    Tree-walk interpreter
  src/linter.rs         Static analysis (unused vars, unnecessary mut, unreachable code, shadowed vars, constant conditions)
  src/codegen.rs        Translators (Nebula → Python, JS, TS, Go, Swift, Kotlin, Rust, C)
  src/ingest.rs         Ingesters (Python, JS, TS, Go, Swift, Kotlin, Rust, C → Nebula) via tree-sitter + canonical normalization
  src/mcp.rs            MCP server (JSON-RPC over stdio) — read_code, write_code, search_code tools
  src/lang_detect.rs    File extension → language detection and dispatch
  src/formatter.rs      Code formatter (AST → canonical Nebula with walrus :=)
  src/main.rs           CLI entrypoint (run, check, lint, fmt, compact, expand, to-*, from-*, mcp, repl)
examples/               Working .neb example programs
bench/                  Token compression benchmark suite
  corpus/               Test programs in 8 languages (equivalent logic)
  run_bench.py          Benchmark runner (tiktoken cl100k_base)
  token_audit.py        Tokenizer audit — checks keyword efficiency
  results.md            Latest benchmark results
```

## Commands

```sh
cd compiler
cargo build                              # Build
cargo test                               # Run all tests (261)
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
cd ../bench && ../.venv/bin/python3 run_bench.py  # Run token compression benchmark
```

## Design Principles

1. **Token-optimal** — Every syntax choice is validated by cl100k_base tokenizer audit. Use the cheapest form: bare assignment over `let`/`mut`, indent blocks over braces, Python-native method names over short aliases (when equal or cheaper).
2. **Consistent** — No special cases. Every construct follows the same patterns.
3. **Translatable** — Any language maps cleanly to/from Nebula. The 8 ingesters produce canonical normalized output.
4. **Multi-paradigm** — FP, OOP, procedural all first-class.
5. **Ceremony-free** — Strip what the target doesn't need. Go pointers, Rust lifetimes, C includes — normalized away in the IL. Languages that don't need `struct`/`impl` (Python, JS) use `class` instead.

## Key Syntax (Nebula 1.0)

- Both short (`fn`, `rt`, `fo`, `lt`, `mt`) and long (`def`, `return`, `for`, `let`, `mut`) keyword forms accepted — all 1 token in cl100k_base
- Bare assignment for mutable (`x = 5`), walrus for immutable (`x := 5`)
- Indent blocks (`:` + dedent) and brace blocks (`{ }`) both valid
- `class` sugar for combined struct+impl
- `|x| expr` lambdas, `{ it.X }` trailing closures
- `and`/`or`/`not` logical ops, `|>` pipe, `??` nil coalesce
- `{}` string interpolation, `..`/`..=` ranges
- `self` for self reference, `True`/`False`/`None` for literals
- `elif` accepted (desugars to `else if`)
- `except` accepted as alias for `catch`
- Newlines as statement terminators (no semicolons)

## When Working on This Project

- Run `cargo test` after every change (auto-runs via PostToolUse hook)
- The spec at `spec/LANGUAGE.md` is the design authority
- **Token-first design** — every syntax choice must be validated by cl100k_base audit. Run `bench/token_audit.py` to verify. Never use a form that costs more tokens than the alternative.
- Zero compiler warnings policy — build must be clean
- All codegen targets in `codegen.rs` follow the same `XxxGen { indent }` struct pattern
- AST enums use exhaustive matching — adding a new variant will cause compiler errors at every match
- All source files are small enough to read in full (~18K lines total across 12 modules)
- The `ingest.rs` pipeline applies: tree-sitter parse → emit Nebula → postprocess → strip semicolons → minify → canonicalize (snake_case, Python-native methods, keyword normalization, walrus, type normalization) → braces_to_indent → language-specific cleanup
