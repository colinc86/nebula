[← Back to README](../README.md)

# Nebula Language Specification v1.0

## Relation to Prior Work

Nebula extends the grammar-redesign approach pioneered by **SimPy** (Sun et al., ISSTA 2024, [arXiv:2404.16333](https://arxiv.org/abs/2404.16333)) and **Token Sugar** (Sun et al., ASE 2025, [arXiv:2512.08266](https://arxiv.org/abs/2512.08266)). Where SimPy preserves Python's AST and replaces only its terminal notations with single-token placeholders, Nebula introduces a universal code IL that maps cleanly across 8 source languages (Python, Go, JS, TS, Rust, Swift, Kotlin, C). Every syntax choice is validated by cl100k_base tokenizer audit — the canonical form always uses the cheapest token representation.

## Design Philosophy

Nebula is designed to be the most LLM-friendly programming language possible. Every design decision prioritizes:

1. **Brevity** — Maximum code density. Every token earns its place. An LLM can load more Nebula into context than equivalent code in any other language.
2. **Consistency** — No special cases, no surprising behavior
3. **Translatability** — Any language maps cleanly to/from Nebula via `nebula translate`
4. **Universality** — Supports functional, OOP, procedural, and mixed paradigms equally
5. **Explicitness** — Code reads like its intent; no hidden magic

## File Extension

`.neb`

## Comments

```nebula
// Single-line comment

/* 
   Multi-line comment 
*/

/// Doc comment (attaches to the next declaration)
```

## Primitive Types

| Type     | Description                  | Literals                        |
|----------|------------------------------|---------------------------------|
| `Int`    | 64-bit signed integer        | `42`, `-7`, `1_000_000`        |
| `Float`  | 64-bit floating point        | `3.14`, `-0.5`, `1.0e10`      |
| `Bool`   | Boolean                      | `True`, `False` (or `tt`, `ff`) |
| `String` | UTF-8 string                 | `"hello"`, `"line\n"`          |
| `Char`   | Single Unicode character     | `'a'`, `'\n'`                  |
| `Nil`    | Absence of value             | `None` (or `nil`)               |

## Variables

```nebula
// Immutable binding (walrus operator — saves 1 token vs `let`)
x := 42
name: String := "Nebula"

// Mutable binding (bare assignment — saves 1 token vs `mut`)
count = 0
count = count + 1

// Tuple unpacking
a, b = 0, 1
a, b = b, a + b

// Long forms also accepted (all keywords are 1 token in cl100k_base)
let x = 42       // same as x := 42
mut count = 0    // same as count = 0
const MAX = 1024 // same as MAX := 1024
```

## Type Inference

Types are inferred wherever possible. Annotations are optional but always allowed:

```nebula
x := 42          // inferred as Int
y: Float := 42   // explicit Float
z := 42.0        // inferred as Float
```

## Strings

```nebula
name := "world"
greeting := "Hello, {name}!"          // interpolation with {}
multi := """
    This is a multi-line
    string with {name} interpolation
"""
raw := r"no \escapes \here"           // raw string
```

## Operators

### Arithmetic
`+`, `-`, `*`, `/`, `%` (modulo), `**` (power)

### Comparison
`==`, `!=`, `<`, `>`, `<=`, `>=`

### Logical
`and`, `or`, `not`

### Bitwise
`&`, `|`, `^`, `~`, `<<`, `>>`

### Assignment
`=` (mutable binding / reassignment), `:=` (immutable binding), `+=`, `-=`, `*=`, `/=`, `%=`

### Pipe
`|>` — Passes the left-hand value as the first argument to the right-hand function:
```nebula
[1, 2, 3] |> map(double) |> filter(is_even) |> sum()
```

### Range
`..` (exclusive), `..=` (inclusive):
```nebula
0..10      // 0 to 9
0..=10     // 0 to 10
```

### Nil Coalescing
`??` — Returns left if not nil, otherwise right:
```nebula
value := maybe_nil() ?? "default"
```

## Control Flow

### If / Else

Both indent and brace forms are valid:

```nebula
// Indent form (canonical)
if x > 0:
    print("positive")
elif x == 0:
    print("zero")
else:
    print("negative")

// Brace form (also valid)
if x > 0 {
    print("positive")
} else if x == 0 {
    print("zero")
} else {
    print("negative")
}

// If as expression (braces required for inline)
abs := if x >= 0 { x } else { -x }
```

### Match (Pattern Matching)

```nebula
match value {
    0 => print("zero")
    1..=9 => print("single digit")
    n if n > 100 => print("big: {n}")
    _ => print("other")
}

// Match as expression
label := match status {
    Status.Ok => "success"
    Status.Error(msg) => "failed: {msg}"
    _ => "unknown"
}
```

### Loops

```nebula
// For loop (indent form)
for item in collection:
    process(item)

// For with index
for i, item in collection:
    print("{i}: {item}")

// While loop
while condition:
    do_something()

// Infinite loop
loop:
    if done:
        break

// Loop as expression (breaks with a value)
result := loop {
    attempt := try_something()
    if attempt.is_ok() { break attempt.unwrap() }
}
```

## Functions

Both `def` and `fn` are accepted (both 1 token in cl100k_base). Both indent and brace blocks work.

```nebula
// Basic function (indent form)
def add(a: Int, b: Int) -> Int:
    a + b    // last expression is the return value

// Short-form: single expression with =
def add(a, b) = a + b
def double(x) = x * 2
def square(x: Int) -> Int = x * x

// With default parameters
def greet(name: Str, greeting = "Hello") -> Str:
    "{greeting}, {name}!"

// Named arguments at call site
greet(name: "Alice", greeting: "Hi")

// Explicit return
def early_return(x: Int) -> Int:
    if x < 0:
        return 0
    x * 2

// Lambda / anonymous function
double := |x| x * 2
add := |a, b| a + b
process := |x| {
    y := x * 2
    y + 1
}

// Generic function
fn first<T>(list: [T]) -> T? {
    if list.is_empty() { None } el { Some(list[0]) }
}

// Variadic parameters
fn sum(numbers: ...Int) -> Int {
    numbers.fold(0, |acc, n| acc + n)
}
```

### Type Shortcuts (Brevity)

These shorthand forms reduce verbosity in type annotations:

| Shorthand  | Full form          | Example              |
|------------|--------------------|----------------------|
| `[T]`      | `List<T>`         | `[Int]`, `[String]` |
| `{K: V}`   | `Map<K, V>`       | `{String: Int}`     |
| `T?`       | `Option<T>`       | `Int?`, `String?`   |
| `(A, B)`   | `Tuple<A, B>`     | `(Int, String)`     |

Both forms are always valid. The shorthand is preferred for brevity.

## Collections

### List

```nebula
lt numbers: List<Int> = [1, 2, 3, 4, 5]
lt first = numbers[0]
lt slice = numbers[1..3]

// List operations
numbers.map(|x| x * 2)
numbers.filter(|x| x > 2)
numbers.fold(0, |acc, x| acc + x)
numbers.any(|x| x > 3)
numbers.all(|x| x > 0)
numbers.find(|x| x == 3)
numbers.sort()
numbers.reverse()
numbers.len()
```

### Map

```nebula
lt scores: Map<String, Int> = {
    "alice": 95,
    "bob": 87,
}

lt alice_score = scores["alice"]     // Option<Int>
scores.keys()
scores.values()
scores.entries()
scores.contains_key("alice")
```

### Set

```nebula
lt unique: Set<Int> = {1, 2, 3}
unique.contains(2)
unique.union(other_set)
unique.intersect(other_set)
```

### Tuple

```nebula
lt pair = (1, "hello")
lt (num, text) = pair    // destructuring
lt first = pair.0
```

## Structs and Classes

```nebula
// Struct with fields (Go/Rust style)
struct Point:
    x: Float
    y: Float

p := Point { x: 1.0, y: 2.0 }
p2 := Point { ...p, x: 3.0 }    // spread with override

// Class style (Python/JS — combines struct + impl, no separate field declaration)
class Circle:
    def new(self, x: Float, y: Float, r: Float):
        self.center = Point { x, y }
        self.radius = r

    def area(self) -> Float:
        return std.math.PI * self.radius ** 2

    def scale(self, factor: Float):
        self.radius *= factor

// Struct + impl style (Go/Rust — separate data and methods)
struct Circle:
    center: Point
    radius: Float

impl Circle:
    def area(self) -> Float:
        return std.math.PI * self.radius ** 2

c := Circle(0.0, 0.0, 5.0)
print(c.area())
```

## Enums

```nebula
// Simple enum
enum Color {
    Red
    Green
    Blue
}

// Enum with associated data (algebraic data types)
enum Shape {
    Circle(radius: Float)
    Rectangle(width: Float, height: Float)
    Point
}

impl Shape:
    def area(self) -> Float:
        match self {
            Shape.Circle(r) => std.math.PI * r ** 2
            Shape.Rectangle(w, h) => w * h
            Shape.Point => 0.0
        }
```

## Traits (Interfaces)

```nebula
trait Printable:
    def to_string(self) -> Str

tr Drawable {
    fn draw(sf)
    
    // Default implementation
    fn draw_at(sf, x: Float, y: Float) {
        translate(x, y)
        sf.draw()
    }
}

// Implementing traits
im Printable fo Circle {
    fn to_string(sf) -> String {
        "Circle(r={sf.radius})"
    }
}

// Trait bounds
fn print_all<T: Printable>(items: List<T>) {
    fo item in items {
        print(item.to_string())
    }
}

// Multiple trait bounds
fn draw_and_print<T: Drawable + Printable>(item: T) {
    item.draw()
    print(item.to_string())
}
```

## Error Handling

```nebula
// Functions that can fail return Result
fn parse_int(s: String) -> Result<Int, String> {
    // ...
}

// The ? operator propagates errors
fn double_parse(s: String) -> Result<Int, String> {
    lt n = parse_int(s)?
    Ok(n * 2)
}

// Try/catch for ergonomic error handling; optional `finally` runs on both paths.
try {
    lt file = open("data.txt")?
    lt data = file.read_all()?
    process(data)
} ch e {
    print("Error: {e}")
} finally {
    cleanup()
}

// Guard clause with `el`
lt n = parse_int(input) el {
    print("Invalid number")
    rt
}
```

## Modules

```nebula
// file: math/geometry.neb
md math.geometry

pb fn distance(a: Point, b: Point) -> Float {
    ((b.x - a.x) ** 2 + (b.y - a.y) ** 2).sqrt()
}

// Visibility: `pb` = public, default = module-private

// file: main.neb
use math.geometry
use math.geometry.{distance, Point}
use std.io.{print, read_line}

// Aliasing
use math.geometry as geo
```

## Closures and Higher-Order Functions

```nebula
fn make_adder(n: Int) -> Fn(Int) -> Int {
    |x| x + n
}

lt add5 = make_adder(5)
print(add5(3))  // 8

// Functions as parameters
fn apply_twice<T>(f: Fn(T) -> T, value: T) -> T {
    f(f(value))
}
```

## Concurrency

```nebula
// Async/await
ac fn fetch_data(url: String) -> Result<String, Error> {
    lt response = aw http.get(url)?
    Ok(response.body)
}

// Spawn concurrent tasks
lt task1 = sp fetch_data("https://api.example.com/a")
lt task2 = sp fetch_data("https://api.example.com/b")
lt (a, b) = aw (task1, task2)
```

## Type Aliases

```nebula
ty UserId = Int
ty Handler = Fn(Request) -> Response
ty StringMap<V> = Map<String, V>
```

## Decorators

```nebula
@memoize
fn fibonacci(n: Int) -> Int {
    if n <= 1 { n } el { fibonacci(n - 1) + fibonacci(n - 2) }
}

@deprecated("Use new_api() instead")
fn old_api() { ... }
```

## Testing

```nebula
#[test]
fn test_addition() {
    assert(add(2, 3) == 5)
    assert_eq(add(0, 0), 0)
    assert_ne(add(1, 1), 3)
}

#[test]
fn test_failure() {
    assert_err(parse_int("not a number"))
}
```

## Standard Library Overview

| Module          | Description                              |
|-----------------|------------------------------------------|
| `std.io`        | Print, read, file I/O                    |
| `std.math`      | Math functions, constants                |
| `std.string`    | String utilities                         |
| `std.collections` | Extended collection types              |
| `std.fs`        | File system operations                   |
| `std.net`       | Networking, HTTP                         |
| `std.json`      | JSON parsing/serialization               |
| `std.time`      | Date, time, duration                     |
| `std.fmt`       | Formatting utilities                     |
| `std.os`        | OS interaction, env vars, processes      |

## Grammar Summary (EBNF sketch)

```ebnf
program        = declaration* ;
declaration    = fn_decl | struct_decl | enum_decl | trait_decl 
               | impl_decl | use_decl | const_decl | statement ;
statement      = let_stmt | mut_stmt | expr_stmt | return_stmt 
               | if_stmt | match_stmt | for_stmt | while_stmt | loop_stmt ;
fn_decl        = "fn" IDENT generics? "(" params? ")" ("->" type)? block ;
struct_decl    = "st" IDENT generics? "{" field_list "}" ;
enum_decl      = "en" IDENT generics? "{" variant_list "}" ;
trait_decl     = "tr" IDENT generics? "{" trait_item* "}" ;
impl_decl      = "im" (IDENT "fo")? IDENT "{" fn_decl* "}" ;
let_stmt       = "lt" pattern (":" type)? "=" expr ;
mut_stmt       = "mt" IDENT (":" type)? "=" expr ;
block          = "{" statement* expr? "}" ;
expr           = pipe_expr ;
pipe_expr      = or_expr ("|>" or_expr)* ;
```

## Reserved Keywords

```
lt mt cn fn rt if el mc fo wl lp bk ct
st en tr im use md pb sf Sf ty ac aw
sp try ch finally tt ff nil and or not in as
```
