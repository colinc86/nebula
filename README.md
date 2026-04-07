# Nebula

**The LLM-friendly programming language.** Maximum code density. Universal translatability. Every token earns its place.

Nebula is designed for AI code generation — programs compress smaller, fit more logic in context, and translate cleanly to any target language.

## Quick Start

```sh
cd compiler && cargo build --release
./target/release/nebula examples/hello.neb        # Run
./target/release/nebula to-py examples/hello.neb   # → Python
./target/release/nebula to-ts examples/hello.neb   # → TypeScript
./target/release/nebula compact examples/hello.neb # Compress
```

## Why Nebula?

| Feature | Nebula | Python | JavaScript |
|---------|--------|--------|------------|
| `let x = 42` | `lt x = 42` | `x = 42` | `const x = 42;` |
| `for i in list` | `fo i in list {` | `for i in list:` | `for (const i of list) {` |
| `if/else` | `if x { } el { }` | `if x:` | `if (x) { } else { }` |
| `function` | `fn f(x) = x*2` | `def f(x): return x*2` | `function f(x) { return x*2; }` |
| `struct` | `st P { x: Int }` | `class P:` (6 lines) | `class P { constructor...` |

**Compacted Nebula is 10-28% smaller than equivalent TypeScript/Go/Rust.**

## Example

```nebula
st User { name: String, age: Int }

im User {
    fn new(name, age) = User { name: name, age: age }
    fn greet(sf) = prt("{sf.name} is {sf.age}")
}

lt users = [User.new("Alice", 30), User.new("Bob", 25)]
lt names = users.mp(|u| u.name).jn(", ")
prt("Users: {names}")
```

## Language Features

- **Short keywords**: `lt` `mt` `fn` `if` `el` `mc` `fo` `wl` `lp` `bk` `ct` `st` `en` `tr` `im` `rt`
- **Types**: Int, Float, Bool, String, Char, Nil, List, Map, Tuple, Struct, Enum
- **FP**: `map`/`mp`, `filter`/`fl`, `fold`/`fd`, `reduce`/`rd`, pipe `|>`, lambdas `|x| expr`
- **OOP**: structs, traits, impl blocks, `sf` (self)
- **Pattern matching**: `mc val { 0 => "zero", 1..=9 => "small", _ => "big" }`
- **Error handling**: `try { } ch e { }`, `?` operator, `??` nil coalesce
- **Destructuring**: `lt (a, b) = (1, 2)`
- **Slicing**: `list[1..3]`, `list[-1]`, `str[0..5]`

## CLI Commands

```
nebula <file>              Run a program
nebula check <file>        Parse-check
nebula lint <file>         Static analysis (5 rules)
nebula fmt <file>          Format to canonical style
nebula compact <file>      Compress for LLM context
nebula expand <file>       Restore from compressed
nebula stats <file>        Density analysis vs other languages
nebula builtins            List all built-in functions & methods
nebula repl                Interactive REPL
```

## Translate To (8 targets)

```
nebula to-py <file>        Python
nebula to-js <file>        JavaScript
nebula to-ts <file>        TypeScript
nebula to-go <file>        Go
nebula to-swift <file>     Swift
nebula to-kt <file>        Kotlin
nebula to-rs <file>        Rust
nebula to-c <file>         C
```

## Translate From (8 sources, tree-sitter powered)

```
nebula from-py <file>      Python → Nebula
nebula from-js <file>      JavaScript → Nebula
nebula from-ts <file>      TypeScript → Nebula
nebula from-go <file>      Go → Nebula
nebula from-swift <file>   Swift → Nebula
nebula from-kt <file>      Kotlin → Nebula
nebula from-rs <file>      Rust → Nebula
nebula from-c <file>       C → Nebula
```

## MCP Server

Nebula includes an MCP (Model Context Protocol) server that works as a transparent compression layer between LLMs and codebases. The LLM reads/writes Nebula; the user's files stay in their original language.

```
nebula mcp                 Start MCP server (JSON-RPC over stdio)
```

Three tools: `read_code` (translate file → Nebula), `write_code` (translate Nebula → file), `search_code` (grep with Nebula results).

## Token Compression Benchmark

Measured with tiktoken cl100k_base on equivalent ~100-line programs:

| Language | Token Compression | Byte Compression |
|----------|------------------|-----------------|
| TypeScript | **14.6%** | 30.6% |
| Kotlin | **13.7%** | 25.0% |
| Swift | **12.8%** | 25.1% |
| Go | **12.5%** | 9.6% |
| Rust | **9.1%** | 22.0% |
| JavaScript | **8.1%** | 27.3% |
| C | **6.0%** | 16.8% |
| Python | **2.4%** | 20.2% |
| **Mean** | **9.9%** | **22.1%** |

Techniques: tokenizer-aware alias selection (arXiv:2512.08266 Token Sugar), LLMLingua-inspired minification (comment/whitespace pruning), tree-sitter AST-based structural conversion.

Full results: [`bench/results.md`](bench/results.md)

## Built-in Aliases

Every built-in has a short form for maximum density:

| Full | Short | Full | Short |
|------|-------|------|-------|
| `print` | `prt` | `map` | `mp` |
| `len` | `ln` | `filter` | `fl` |
| `to_string` | `tsr` | `fold` | `fd` |
| `to_int` | `toi` | `sort` | `srt` |
| `to_float` | `tof` | `join` | `jn` |
| `range` | `rng` | `push` | `psh` |

## Project Structure

```
compiler/src/
  lexer.rs          Tokenizer
  parser.rs         Recursive descent parser
  ast.rs            Abstract syntax tree
  interpreter.rs    Tree-walk interpreter
  linter.rs         Static analysis
  codegen.rs        8 language translators (Nebula → target)
  ingest.rs         8 language ingesters (source → Nebula, tree-sitter)
  mcp.rs            MCP server (JSON-RPC over stdio)
  lang_detect.rs    File extension → language detection
  formatter.rs      Code formatter
  compactor.rs      LLM context compressor
  main.rs           CLI
bench/              Token compression benchmark suite
examples/           17 working example programs
spec/               Language specification
```
