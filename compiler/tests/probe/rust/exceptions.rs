use std::num::ParseIntError;

// ? operator for propagation
fn parse_and_double(s: &str) -> Result<i32, ParseIntError> {
    let n: i32 = s.parse()?;
    Ok(n * 2)
}

// custom error
#[derive(Debug)]
enum MyErr {
    Empty,
    NotANumber(String),
}

fn parse_strict(s: &str) -> Result<i32, MyErr> {
    if s.is_empty() {
        return Err(MyErr::Empty);
    }
    s.parse::<i32>().map_err(|_| MyErr::NotANumber(s.to_string()))
}

// panic
fn must_positive(n: i32) -> i32 {
    if n < 0 {
        panic!("negative: {}", n);
    }
    n
}

// unwrap / expect
fn first_or_zero(xs: &[i32]) -> i32 {
    *xs.first().unwrap_or(&0)
}
