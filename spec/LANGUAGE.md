# Nebula Language Specification v0.2

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
| `Bool`   | Boolean                      | `tt`, `ff`                      |
| `String` | UTF-8 string                 | `"hello"`, `"line\n"`          |
| `Char`   | Single Unicode character     | `'a'`, `'\n'`                  |
| `Nil`    | Absence of value             | `nil`                           |

## Variables

```nebula
// Immutable binding (default)
lt x = 42
lt name: String = "Nebula"

// Mutable binding
mt count = 0
count = count + 1

// Constants (compile-time)
cn MAX_SIZE = 1024
```

## Type Inference

Types are inferred wherever possible. Annotations are optional but always allowed:

```nebula
lt x = 42          // inferred as Int
lt y: Float = 42   // explicit Float
lt z = 42.0        // inferred as Float
```

## Strings

```nebula
lt name = "world"
lt greeting = "Hello, {name}!"          // interpolation with {}
lt multi = """
    This is a multi-line
    string with {name} interpolation
"""
lt raw = r"no \escapes \here"           // raw string
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
`=`, `+=`, `-=`, `*=`, `/=`, `%=`

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
lt value = maybe_nil() ?? "default"
```

## Control Flow

### If / Else

```nebula
if x > 0 {
    print("positive")
} el if x == 0 {
    print("zero")
} el {
    print("negative")
}

// If as expression
lt abs = if x >= 0 { x } el { -x }
```

### Match (Pattern Matching)

```nebula
mc value {
    0 => print("zero")
    1..=9 => print("single digit")
    n if n > 100 => print("big: {n}")
    _ => print("other")
}

// Match as expression
lt label = mc status {
    Status.Ok => "success"
    Status.Error(msg) => "failed: {msg}"
    _ => "unknown"
}
```

### Loops

```nebula
// Infinite loop
lp {
    if done { bk }
}

// While loop
wl condition {
    do_something()
}

// For loop (iterates over any Iterable)
fo item in collection {
    process(item)
}

// For with index
fo i, item in collection {
    print("{i}: {item}")
}

// Loop as expression (breaks with a value)
lt result = lp {
    lt attempt = try_something()
    if attempt.is_ok() { bk attempt.unwrap() }
}
```

## Functions

```nebula
// Basic function
fn add(a: Int, b: Int) -> Int {
    a + b    // last expression is the return value
}

// Short-form: single expression with =
fn add(a, b) = a + b
fn double(x) = x * 2
fn square(x: Int) -> Int = x * x

// With default parameters
fn greet(name: String, greeting = "Hello") -> String {
    "{greeting}, {name}!"
}

// Named arguments at call site
greet(name: "Alice", greeting: "Hi")

// Explicit return
fn early_return(x: Int) -> Int {
    if x < 0 { rt 0 }
    x * 2
}

// Lambda / anonymous function
lt double = |x| x * 2
lt add = |a, b| a + b
lt process = |x| {
    lt y = x * 2
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

## Structs (Data Types)

```nebula
st Point {
    x: Float
    y: Float
}

lt p = Point { x: 1.0, y: 2.0 }
lt p2 = Point { ...p, x: 3.0 }    // spread with override

// Struct with methods (OOP style)
st Circle {
    center: Point
    radius: Float
}

im Circle {
    fn new(x: Float, y: Float, r: Float) -> Circle {
        Circle { center: Point { x, y }, radius: r }
    }

    fn area(sf) -> Float {
        std.math.PI * sf.radius ** 2
    }

    fn scale(mt sf, factor: Float) {
        sf.radius *= factor
    }
}

lt c = Circle.new(0.0, 0.0, 5.0)
print(c.area())
```

## Enums

```nebula
// Simple enum
en Color {
    Red
    Green
    Blue
}

// Enum with associated data (algebraic data types)
en Shape {
    Circle(radius: Float)
    Rectangle(width: Float, height: Float)
    Point
}

im Shape {
    fn area(sf) -> Float {
        mc sf {
            Shape.Circle(r) => std.math.PI * r ** 2
            Shape.Rectangle(w, h) => w * h
            Shape.Point => 0.0
        }
    }
}

// Option and Result are built-in enums
en Option<T> {
    Some(T)
    None
}

en Result<T, E> {
    Ok(T)
    Err(E)
}
```

## Traits (Interfaces)

```nebula
tr Printable {
    fn to_string(sf) -> String
}

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

// Try/catch for ergonomic error handling
try {
    lt file = open("data.txt")?
    lt data = file.read_all()?
    process(data)
} ch e {
    print("Error: {e}")
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
sp try ch tt ff nil and or not in as
```
