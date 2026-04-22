// sum type with data
enum Shape {
    Circle { r: f64 },
    Square(f64),
    Rect { w: f64, h: f64 },
}

fn area(s: &Shape) -> f64 {
    match s {
        Shape::Circle { r } => 3.14159 * r * r,
        Shape::Square(side) => side * side,
        Shape::Rect { w, h } => w * h,
    }
}

// C-style enum
#[derive(Copy, Clone, PartialEq)]
enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

fn level_name(l: Level) -> &'static str {
    match l {
        Level::Debug => "debug",
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Error => "error",
    }
}

// enum with discriminants
enum Exit {
    Ok = 0,
    Fail = 1,
}
