fn greet(name: &str, age: i32) -> String {
    format!("hi {}, you are {}", name, age)
}

fn fmt_float(x: f64) -> String {
    format!("{:.2}", x)
}

fn debug_pair(a: i32, b: i32) -> String {
    format!("sum={} prod={}", a + b, a * b)
}

// positional + named args
fn reorder(a: i32, b: i32) -> String {
    format!("{1} then {0}", a, b)
}

fn named(name: &str) -> String {
    format!("hi, {name}")
}

// Display trait
use std::fmt;

struct Point { x: i32, y: i32 }

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({},{})", self.x, self.y)
    }
}
