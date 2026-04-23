# Nebula Round-Trip Probe — Triage Report

Scanner: `bench/scripts/probe_ingesters.py`
Catalog: `bench/scripts/probe_catalog.json`
Probes: `compiler/tests/probe/<lang>/*.<ext>` (91 files across 8 languages)
Pipeline per probe: (1) `from-<lang>` ingest, (2) `nebula check` on the Nebula, (3) `to-<lang>` round-trip, (4) native parse/compile of the round-tripped output.

## Summary

| lang       | total | clean | minor | major | critical |
|------------|------:|------:|------:|------:|---------:|
| c          |    10 |     0 |     0 |     4 |        6 |
| go         |    13 |     0 |     0 |     7 |        6 |
| javascript |    10 |     3 |     0 |     1 |        6 |
| kotlin     |    12 |     0 |     0 |     3 |        9 |
| python     |    13 |     4 |     0 |     3 |        6 |
| rust       |    11 |     0 |     0 |     1 |       10 |
| swift      |    11 |     3 |     0 |     0 |        8 |
| typescript |    11 |     3 |     0 |     0 |        8 |
| **total**  |  **91** | **13** | **0** | **19** | **59** |

Severity: **critical** = phase 1/2/3 failure (ingest produced un-parseable Nebula, or round-trip codegen crashed). **major** = phases 1-3 ok but phase 4 native compile failed. **minor** = antipatterns only. **clean** = all four phases green and no antipatterns.

Only 13/91 probes (14%) survive the full round trip. Every language has critical ingest/check gaps. Swift, Rust, Kotlin, TypeScript never survive phase 4 once phase 3 succeeds, because the only clean probes in those languages are the ones where phase 4 happened to not trip any remaining codegen issues (slices, string interpolation, optionals).

## Cross-language idiom health

Idioms ranked by (critical + major) count across languages:

| idiom              | crit | maj | clean | total probes |
|--------------------|-----:|----:|------:|-------------:|
| iteration          |   5  |  2  |   1   |     8 |
| typed_collections  |   5  |  0  |   1   |     6 |
| exceptions         |   6  |  0  |   0   |     6 |
| closures           |   4  |  2  |   0   |     6 |
| string_format      |   4  |  0  |   1   |     5 |
| generics           |   4  |  0  |   0   |     4 |
| qualified_types    |   4  |  0  |   0   |     4 |
| tuples             |   4  |  0  |   0   |     4 |
| enums              |   3  |  0  |   1   |     4 |
| async              |   3  |  0  |   0   |     3 |
| pattern_matching   |   2  |  0  |   0   |     2 |
| slices             |   2  |  1  |   4   |     7 |

No idiom has a clean sweep across all 8 languages. Slices are the healthiest (4/7 clean), but still fail in Go (`xs[1..]` lowering is wrong) and Rust (`&xs[..3]` not accepted by the Nebula grammar).

## Top failure patterns (by frequency)

Below are the dominant error signatures grouped by what the symptom implies.

### 1. `Colon` appears where a brace/value was expected (14 cases)
Ingesters emit `:` to open indent blocks in contexts where the parser expects a braced block, an expression, or a type. Examples:
- `c/errors` L5 `n: Int` inside what looks like a struct/fn param body.
- `c/string_format` L8 `ap: va_list` — varargs state is emitted as a typed decl rather than `va_start`.
- `kotlin/tuples` L8 `impl Point:` — `impl` header should take `{ ... }` but the ingester emitted `:` indent.
- `swift/exceptions` L1 `enum BadInput: Error:` — conformance `: Error` is re-using the colon that opens the block.
- `swift/pattern_matching` L6 `match s:` — match arm uses `:` instead of required brace block.

Root cause: mixed block-opener convention. Ingesters inconsistently pick `:` vs `{ }`, and the Nebula parser is stricter than the pretty-printer assumes.

### 2. `LeftBracket` / `LeftParen` / `LeftBrace` mismatch in type positions (≈26 cases)
Many signatures with shape `Expected X, got Y` where Y is a bracket character:
- `Expected identifier, got LeftParen` (6) — `rust/iteration` `xs.enumerate().map(|(i, x)| (i, x))` — closure param destructuring `|(i,x)|` becomes bare `(` after `|`.
- `Expected identifier, got LeftBracket` (4) — `rust/async` `ids: List) -> [String]` — slice-as-return type `[String]` leaks from `Vec<String>` normalization.
- `Expected LeftParen, got LeftBracket` (4) — `swift/generics` `def max_of[T: Comparable](...)` — Swift generic bracket `<T>` becomes `[T]` in Nebula fn header, which the parser rejects.
- `Expected LeftBrace, got LeftBracket` (2) — `go/generics` `impl Stack[T]:` — generic impl headers end up with `[T]` where parser wants a block.
- `Expected identifier, got LeftBrace` (4) — `rust/enums` `Circle { r: Float }` inside `enum` — struct-variant bodies not in the grammar.
- `Expected LeftBrace, got Greater` (2) — `go/typed_collections` `List>:` — unbalanced generic depth (`>` at end of `[][]int` lowering).
- `Expected Colon, got LeftBrace` (2) — `c/unions_and_bitfields` `union:` — bare `union` block emitted without a tag.

Root cause mix: (a) generics carrier — Nebula apparently uses `[]` for generic type args but some ingesters emit `<>` or `[]` inconsistently; (b) grammar gap for struct-shape enum variants.

### 3. Qualified type paths lose their dots (6 cases, all `Expected LeftBrace, got Dot`)
Return/param types like `java.util.List`, `np.ndarray`, `fmt.Display`, `io.Reader` (Swift/Kotlin/Python/Rust) land in Nebula unchanged, but the parser rejects `->` followed by a dotted identifier path. Every language that has namespaced types regresses here.

Affected probes: `kotlin/qualified_types`, `python/qualified_types`, `rust/string_format` (`impl fmt.Display fo Point`), `rust/qualified_types` (`use std.io.{self, read, write}` — also a `SelfValue` keyword collision).

Nebula grammar gap: the type grammar does not accept dotted module paths. Either (a) add dotted path support, or (b) the ingesters must flatten to a single identifier before emit.

### 4. Walrus LHS rejects destructuring patterns (4 cases)
`Left side of := must be an identifier or tuple` fires on `[h, t] := xs` (JS destructuring) and `[q, r] := divmod(10, 3)` (TS tuples). The grammar accepts tuple-on-LHS but not array-pattern-on-LHS. Ingesters should either emit `(h, t) := ...` or a plain `=` assignment.

Affected: `javascript/destructuring`, `javascript/iteration`, `typescript/tuples`, `python/pattern_matching` (similar shape).

### 5. Backtick/template-literal leaks (4 cases)
`Unexpected character: '` ` '` in `javascript/exceptions`, `typescript/exceptions`, `javascript/template_literals`. The JS/TS ingesters preserve backtick template literals verbatim even though Nebula strings use double quotes and `{}` interpolation. Multi-line template literals additionally trip an "Unterminated string" error.

### 6. `or` lexed as a keyword inside identifiers (4 cases)
`javascript/closures`, `typescript/closures`: pretty-print output contains `inc: or += 1count, get: or count` — the `++count` operator is being rewritten as `or += 1` and the identifier `count` is concatenated with the number `1`. This is a JS/TS ingester bug where `++x` is mis-normalized to `or <assign>` (probably a botched unary-operator rewrite). The resulting text is not just un-parseable Nebula; the semantics are lost.

### 7. Kotlin `until`/`downTo`/`step` range ops leak verbatim (2 cases)
`kotlin/iteration` L3 `for i in 0 until n total += i` — `until` kept as an identifier, and the newline between the range and body was swallowed. `kotlin/slices` similarly leaves `0 until 10`, `10 downTo 0 step 2` as raw identifiers. Grammar has no binary `until`/`step` operator; ingester should rewrite to `0..n` / `..` `step_by(...)`.

### 8. Reserved-word identifier collisions (6 cases)
- `python/enums`: `use enum.{enum, int_enum, auto}` — `enum` clashes with the Nebula keyword.
- `python/generics`: `-> None:` where `None` is a literal, not a type → `Expected identifier, got Nil`.
- `c/vtable_interface`: `ty Shape = struct Shape Shape` — `struct` keyword used as identifier.
- `kotlin/enums`: `enum st Color(...)` — ingester emits `st` (keyword `struct` short form) after `enum`.
- `rust/qualified_types`: `use std.io.{self, read, write}` — `self` collides with `SelfValue` keyword.
- `typescript/qualified_types`: `interface Point { x: Int; y: Int; }` inside another block where bare `interface` keyword is not allowed.

### 9. Struct-variant enums not expressible (4 cases)
Rust's `enum Shape { Circle { r: f64 }, Rect { w, h } }` and Swift's `enum Shape { case circle(r: Double) }` both hit `Expected identifier, got LeftBrace` / `got Colon`. Nebula's enum grammar accepts tuple variants but not struct-shaped variants. This is a true IL expressiveness gap.

### 10. Function-type parameter syntax (2+ cases)
- `c/function_pointers`: `def map_int(xs: Int, n: Int, (f)(int) -> Int):` — C function-pointer typedef is emitted as a parenthesized anonymous function type the parser cannot handle.
- `go/exceptions`: `def safe(fn_: Fn()) -> error:` — `Fn()` bare-type without `->` trips `Expected Arrow, got RightParen`.
- `javascript/switch_patterns`: `n == 0 =>:` — the JS switch-on-true idiom renders a fat-arrow guard that parser rejects (`Expected FatArrow, got Eq`).

## Phase-4 only failures (major)

These 19 probes parse and round-trip cleanly, but the emitted native code does not compile. They point at codegen bugs rather than IL gaps.

| lang | probe | symptom |
|------|-------|---------|
| c | arrays / iteration | `for (int64_t i = 0; ...)` emitted as bare `i = 0LL;` without the `for (` keyword prefix — loop header is missing. |
| c | control_flow | `return if (...)` — `if` used as expression where C needs a statement. Also types `List` leak. |
| c | structs_and_pointers | `Str* name;` — `String` is emitted as `Str*` but no typedef exists. |
| go | channels | channel-direction arrow dropped: `chan int` instead of `<-chan int`; `go func() { ... }()` goroutine launcher becomes `go func() interface{} { return }()`. |
| go | closures | `t *testing.T` becomes `TestingT` identifier; `run` and `process` become undefined since imports/package scope is lost. |
| go | enums_iota | top-level `const (...)` lost; constants leak outside any declaration → `non-declaration statement outside function body`. |
| go | interfaces | interface method signatures lowered to `List` (the Nebula collection alias) instead of `[]byte`. |
| go | iteration | `for { ... }` infinite-loop form emitted outside a function (likely a stray stmt after round-trip). |
| go | pointers | `*p = 0` becomes `p = 0` — deref operator dropped in codegen. |
| go | tuple_returns | qualified type `io.Reader` becomes identifier `IoReader` (camel-cased, dot dropped). Named returns `(head, tail string)` become undefined. |
| javascript | classes | `super(name)` called inside non-constructor method because `constructor` keyword was lowered to a plain `speak`-style method. |
| kotlin | coroutines_and_flows | bare `import` keyword with no path. Suspend fn and flow-builder don't round-trip. |
| kotlin | data_classes_and_when | extension receiver lost: `fun String.shout()` becomes `fun shout()` with `this.uppercase()` inside but no receiver type. |
| kotlin | slices | `xs.take(3)` emitted, but the re-ingested Nebula collapses the List type and Kotlin rejects `take` (likely because the List type is erased). Missing return statement. |
| python | async_and_context | after round-trip, `async def` loses the `async` modifier; `async with` becomes `with`; indent is off (`try:` double-indented). |
| python | closures | `nonlocal\n  n` split across two lines because the ingester normalizes whitespace and the codegen doesn't join keyword+ident. |
| python | fstrings | `f"{x:.2f}"` becomes `f"{x: .2f}"` (spurious space); `f"{name=}"` becomes `f"{name?}"` (debug specifier turned into something else). |
| rust | traits_and_generics | Every closure body rendered as `(move \|\| /* TODO */);` — codegen leaks a `/* TODO */` placeholder for closure bodies, confirming the TODO-leak scanner rule is live. `format!` args also malformed (`format!("{} barks", self.name)` ok but `first_positive` body becomes `xs.find(\|andx\| (x > 0))` — `&&x` pattern lowered to identifier `andx`). |

Five Python probes show the ast-parse harness importing a module; those errors all point at genuine Python codegen bugs, not scanner hygiene.

## IL grammar gaps vs. ingester/codegen bugs

### Likely grammar gaps (need spec work, not just translator fixes)
1. **Dotted type paths in annotations** (`np.ndarray`, `io.Reader`, `java.util.List`). Impacts 6+ probes across 4 languages.
2. **Struct-shape enum variants** (`Circle { r: f64 }`). Rust and Swift both need this; no workaround without a real grammar extension.
3. **Closure-param destructuring** (`|(i, x)|`). Rust iterators, Swift `.map { (i, x) in ... }` and similar all want this. The parser currently reads `|` `(` as two tokens.
4. **Array-pattern on walrus LHS** (`[h, *t] := xs`). Destructuring forms from Python `match` / JS / TS. Grammar accepts tuple-LHS but not array-pattern-LHS.
5. **Variadic tuple types** (`[T, ...R]`), **labeled tuple types** (`(lo: Int, hi: Int)`), and **generic carrier `[T]` vs. `<T>`** inconsistency. Parser flips between `<>` and `[]` and ingesters sometimes emit the wrong one for the context (fn header vs. type arg).
6. **Named-arg syntax inside calls** (`xs.joinToString(prefix = "[", ...)`). Kotlin and Python both use this, and Nebula treats `=` inside args as assignment rather than kwarg binding.
7. **Backtick/template-literal string form**. JS/TS ingesters preserve backticks because Nebula doesn't have a lossless 1:1 target for multi-line interpolated strings.

### Ingester bugs (codegen → Nebula is wrong)
1. **JS/TS `++x` / `--x`** → emitted as `or += 1` / `or -= 1` with identifier concatenation. `javascript/closures`, `typescript/closures`. Breaks the output outright.
2. **Keyword-reserved identifiers** not renamed: `enum`, `self`, `struct` appear in user-land positions (`python/enums`, `rust/qualified_types`, `c/vtable_interface`).
3. **Kotlin range operators** (`until`, `downTo`, `step`) leaked as raw identifiers.
4. **Go channel direction** arrows stripped (`<-chan` → `chan`).
5. **Block-opener mixing** — `:` used where parser wants `{` (kotlin `impl`, swift `match`, c `union`, swift `enum` conformance header).
6. **Nebula scanner** false-splits `nonlocal n` into two lines in python/closures output.

### Codegen bugs (Nebula → target is wrong)
1. **Rust closure body → `/* TODO */`** for any non-trivial closure (iter 15+: every `move ||` body becomes `/* TODO */`).
2. **Python f-string format spec** mangled: `:.2f` → `: .2f`; `f"{name=}"` → `f"{name?}"`.
3. **C `for` header lost**: `for (int i = 0; i < n; i++)` → `i = 0LL; while (i < n) { ... }` emitted at statement level with `i` undeclared.
4. **C `return if (...)`** — `if` used as expression.
5. **Go** drops package-level `const (...)` grouping, drops receiver pointer `*p = 0` → `p = 0`, and kebabs `io.Reader` → `IoReader`.
6. **Kotlin** extension receiver `String.shout()` loses its receiver type after round-trip; `import` lines emitted empty.
7. **JS** constructor not recognized on round-trip: `speak()` gets a stray `super(name)` call.
8. **TS** (phase 4 not reached — phase 3 still fails on most TS probes).

## Closing observations

- **No language round-trips cleanly on more than half its probes.** Swift and TypeScript both stop at 3/11 clean; Rust is 0/11 despite having a well-scoped probe set.
- **The 10 most valuable immediate fixes are in the list above**, with the grammar gaps (items 1-7 under "Likely grammar gaps") providing leverage across multiple languages and the codegen bugs (Rust closures → TODO, Python f-strings, C for-header, Go const grouping) fixing most remaining majors.
- The scanner's antipattern detectors fire rarely (10 total flags, mostly `casing_variants` and `unclosed_open` / `mismatched_bracket`). Nearly every real failure is caught at phase 2 (`nebula check`) or phase 4 (native compile). Antipatterns may be more useful once ingest output is itself parseable.
