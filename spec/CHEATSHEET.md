[← Back to README](../README.md) · [Full spec →](LANGUAGE.md)

# Nebula Cheat Sheet

One-page reference for Nebula 1.0. Every keyword is one `cl100k_base` token; both short and long forms are accepted aliases.

## Bindings

```nebula
x := 42                  // immutable (walrus, 1 tok vs `let x = 42` = 3 tok)
count = 0                // mutable (bare assignment)
count = count + 1

name: String := "Nebula" // with type annotation
a, b = 0, 1              // tuple assignment
a, b = b, a + b          // tuple swap

// Long forms also accepted (all 1 token):
let x = 42               // same as x := 42
mut count = 0            // same as count = 0
const MAX = 1024         // same as MAX := 1024 (with conventional caps)
```

## Primitives

| Type | Literals |
|---|---|
| `Int` | `42`, `-7`, `1_000_000` |
| `Float` | `3.14`, `1.0e10` |
| `Bool` | `True`, `False` |
| `String` | `"hello"`, `"hi {name}"` (interpolation) |
| `Char` | `'a'`, `'\n'` |
| `Nil` | `None` |

## Functions

```nebula
fn add(a, b) = a + b         // short form (expression body)
def add(a, b): return a + b  // long form (same tokens)

fn greet(name, msg = "Hi"):  // indent block, default arg
    prt("{msg}, {name}")

|x| x * 2                    // lambda
|a, b| a + b                 // multi-arg lambda
xs.map { it * 2 }            // trailing closure (implicit `it`)
```

## Control flow

```nebula
if x > 0: "pos" else: "neg"    // if expression (also `elif`)
if x > 0 { "pos" } else { "neg" }  // brace blocks also valid

match val:                      // exhaustive match
    0 => "zero"
    1..=9 => "small"
    n if n > 100 => "big: {n}"
    _ => "other"

for x in items: prt(x)          // for loop
for i, x in items.enumerate(): prt("{i}: {x}")
for k, v in map: prt("{k}={v}")

while cond: ...                 // while
loop:                           // infinite loop
    if done: break

try: risky()                    // try / catch (also `except`)
except e: prt(e)
finally: cleanup()              // optional; runs on all paths
```

## Types

```nebula
class Point:                    // class sugar (struct + impl combined)
    x: Float
    y: Float
    fn new(x, y) = Point { x, y }
    fn dist(self) = (self.x**2 + self.y**2).sqrt()

struct Point { x: Float, y: Float }  // plain struct (no methods)
impl Point:
    fn new(x, y) = Point { x, y }

enum Shape:
    Circle(r: Float)
    Rect(w: Float, h: Float)
    Dot

trait Drawable:
    fn draw(self)

impl Drawable for Shape:
    fn draw(self): ...
```

## Collections & ranges

```nebula
[1, 2, 3]                   // list
{"a": 1, "b": 2}            // map
(1, "hello")                // tuple
0..10                       // range (exclusive)
0..=10                      // range (inclusive)
```

## Operators

| Group | Operators |
|---|---|
| Arithmetic | `+ - * / % **` |
| Comparison | `== != < > <= >=` |
| Logical (short-circuit) | `and or not` |
| Bitwise | `& \| ^ ~ << >>` |
| Pipe · nil-coalesce · try | `\|>` · `??` · `?` |

## Indexing & slicing

```nebula
list[0]         // index (0-based)
list[-1]        // last element
list[1..3]      // slice [1, 2]
list[1..=3]     // slice [1, 2, 3]
str[0..3]       // string slice
list[i] = val   // mutable index assignment
```

## Common built-ins

```
print  len  type_of  to_string  to_int  to_float  range
abs  sqrt  min  max  floor  ceil  round  assert  assert_eq
read_line  enumerate  zip
```

## Common collection methods

List: `.map .filter .fold .reduce .sort .reverse .unique .count .join .push .contains .first .last .take .skip .sum .min .max .flatten .flat_map .each .zip .enumerate .len .is_empty`

String: `.len .contains .split .trim .to_upper .to_lower .replace .starts_with .ends_with .index_of .substring .chars .rev .repeat .is_empty`

Map: `.keys .values .entries .contains_key .get .len .is_empty`

---

Anything non-obvious is in the [full specification](LANGUAGE.md).
