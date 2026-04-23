fn greet(name: &str) -> String {
    format!("hello, {}", name)
}

fn main() {
    println!("{}", greet("world"));
}
