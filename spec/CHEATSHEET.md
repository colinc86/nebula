# Nebula Cheat Sheet

Nebula uses short keywords for maximum LLM context density.

## Keywords
```
lt = let    mt = mut    cn = const   fn = fn     rt = return
if = if     el = else   mc = match   fo = for    wl = while
lp = loop   bk = break  ct = continue st = struct en = enum
tr = trait   im = impl   pb = pub    sf = self   Sf = Self
ty = type   ac = async  aw = await   sp = spawn  ch = catch
tt = true   ff = false  nil and or not in as use try
```

## Syntax
```nebula
lt x = 42                    // immutable binding
mt y = 0                     // mutable binding
fn add(a, b) = a + b         // short-form function
fn greet(name, msg = "Hi") { // block function with default param
    prt("{msg}, {name}")
}
|x| x * 2                   // lambda
|a, b| a + b                // multi-param lambda
lt (a, b) = (1, 2)          // destructuring
```

## Control Flow
```nebula
if x > 0 { "pos" } el { "neg" }    // if expression
mc val {                             // match
    0 => "zero"
    1..=9 => "small"
    n if n > 100 => "big: {n}"
    _ => "other"
}
fo x in items { prt(x) }            // for loop
fo i, x in items { prt("{i}: {x}") } // with index
fo k, v in map { prt("{k}={v}") }    // map iteration
wl cond { ... }                      // while
lp { if done { bk } }               // loop
try { risky() } ch e { prt(e) }     // try/catch
```

## Types
```nebula
st Point { x: Float, y: Float }     // struct
im Point {                           // impl block
    fn new(x, y) = Point { x, y }
    fn dist(sf) = sqrt(sf.x**2 + sf.y**2)
}
en Shape { Circle(r: Float), Rect(w: Float, h: Float), Dot }
tr Drawable { fn draw(sf) }
im Drawable fo Shape { fn draw(sf) { ... } }
```

## Collections
```nebula
[1, 2, 3]                // list
{"a": 1, "b": 2}         // map
(1, "hello")              // tuple
0..10                     // range (exclusive)
0..=10                    // range (inclusive)
```

## Operators
```
+ - * / % **             // arithmetic
== \!= < > <= >=          // comparison
and or not               // logical (short-circuit)
& | ^ ~ << >>           // bitwise
|> ?? ?                  // pipe, nil-coalesce, try
```

## Built-in Functions (with short aliases)
```
print/prt  len/ln  type_of/typ  to_string/tsr  to_int/toi
to_float/tof  range/rng (1 or 2 args)  abs  sqrt  min  max
floor  ceil  round  assert  assert_eq  assert_ne  read_line
enumerate  zip
```

## List Methods (with short aliases)
```
.map/mp  .filter/fl  .fold/fd  .reduce/rd  .sort/srt  .reverse/rv
.unique/unq  .count/cnt  .join/jn  .push/psh  .contains/con
.first  .last  .take  .skip  .sum  .min  .max  .flatten
.flat_map  .each  .zip  .enumerate  .len/ln  .is_empty
```

## String Methods
```
.len  .contains  .split  .trim  .to_upper  .to_lower  .replace
.starts_with  .ends_with  .index_of  .substring  .chars  .rev
.repeat  .is_empty
```

## Map Methods
```
.keys  .values  .entries  .contains_key/ck  .get  .len  .is_empty
```

## Indexing & Slicing
```nebula
list[0]         // index (0-based)
list[-1]        // last element
list[1..3]      // slice [1,2]
list[1..=3]     // slice [1,2,3]
str[0..3]       // string slice
list[i] = val   // index assignment (mt only)
```
