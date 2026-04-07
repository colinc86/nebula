---
name: add-target
description: Guide for adding a new codegen target language (e.g., C, Bash, C#)
user_invocable: true
---

# Add a New Code Generation Target

You are adding a new target language for Nebula's translator. All existing targets follow an identical pattern.

## Reference
Use `PythonGen` in `compiler/src/codegen.rs` as your model — it's the simplest. Read it fully before starting.

## Steps

### 1. Create the generator struct (`compiler/src/codegen.rs`)

```rust
struct XxxGen {
    indent: usize,
}

impl XxxGen {
    fn ind(&self) -> String {
        // Use target language's conventional indentation
        "    ".repeat(self.indent)
    }

    fn gen_program(&mut self, program: &Program) -> String { ... }
    fn gen_statement(&mut self, stmt: &Statement) -> String { ... }
    fn gen_expr(&mut self, expr: &Expr) -> String { ... }
    // Add helpers as needed (gen_block, gen_params, etc.)
}
```

Every `match` arm in `gen_statement` and `gen_expr` must be covered — handle all AST variants. Check `PythonGen` for the complete list.

### 2. Add the public entry point (top of `codegen.rs`)

```rust
pub fn to_xxx(program: &Program) -> String {
    let mut gen = XxxGen { indent: 0 };
    gen.gen_program(program)
}
```

### 3. Wire into CLI (`compiler/src/main.rs`)

Add to the command match (~line 43):
```rust
"to-xxx" => translate_to(&source, file_path, "xxx"),
```

Add to the `translate_to` function match (~line 158):
```rust
"xxx" => codegen::to_xxx(&program),
```

Update the usage string in the error case (~line 57).

### 4. Add tests (bottom of `codegen.rs`)

Follow the existing test pattern — there are 44+ codegen tests as reference. Test:
- Variable declarations
- Functions and function calls
- Control flow (if/else, for, while, match)
- Structs and methods
- String interpolation
- List/map operations
- Lambdas and closures
- Pipe operator translation

### 5. Language-specific considerations

Think about how Nebula constructs map to this target:
- **String interpolation** — f-strings, template literals, fmt.Sprintf, etc.
- **Lambdas** — arrow functions, blocks, closures
- **Pattern matching** — switch/case, when, if/else chains
- **First-class functions** — function pointers, delegates, std::function
- **Nil handling** — null, None, nil, Optional
- **List operations** — .map/.filter may need imports or polyfills

## After implementation
1. `cargo test` — all tests pass, zero warnings
2. Test with all example files: `for f in examples/*.neb; do cargo run -- to-xxx "$f"; done`
3. Verify the generated code actually runs in the target language if possible
