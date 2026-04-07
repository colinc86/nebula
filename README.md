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

## Translate From

```
nebula from-py <file>      Python → Nebula
nebula from-js <file>      JavaScript/TypeScript → Nebula
nebula from-go <file>      Go → Nebula
```

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
  codegen.rs        8 language translators
  formatter.rs      Code formatter
  compactor.rs      LLM context compressor
  from_python.rs    Python → Nebula converter
  from_js.rs        JavaScript/TypeScript → Nebula converter
  from_go.rs        Go → Nebula converter
  main.rs           CLI
examples/           14 working example programs
spec/               Language specification + cheat sheet
```
