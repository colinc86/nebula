use std::fmt::Display;

trait Speak {
    fn speak(&self) -> String;
}

struct Dog {
    name: String,
}

impl Speak for Dog {
    fn speak(&self) -> String {
        format!("{} barks", self.name)
    }
}

// generic function with trait bound
fn greet<T: Display>(x: T) -> String {
    format!("hi, {}", x)
}

// Result + Option
fn parse_num(s: &str) -> Result<i64, String> {
    s.parse::<i64>().map_err(|e| e.to_string())
}

fn first_positive(xs: &[i32]) -> Option<i32> {
    xs.iter().find(|&&x| x > 0).copied()
}

// match with guards
fn classify(n: i32) -> &'static str {
    match n {
        0 => "zero",
        n if n > 0 => "positive",
        _ => "negative",
    }
}

// closure with move
fn make_counter() -> impl FnMut() -> i32 {
    let mut n = 0;
    move || { n += 1; n }
}
