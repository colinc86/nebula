fn divmod(a: i32, b: i32) -> (i32, i32) {
    (a / b, a % b)
}

fn named_pair(a: i32, b: String) -> (i32, String) {
    (a, b)
}

fn destructure_call() -> i32 {
    let (q, r) = divmod(10, 3);
    q + r
}

// triple
fn coords() -> (i32, i32, i32) {
    (1, 2, 3)
}

// tuple struct
struct Point(i32, i32);

fn origin() -> Point {
    Point(0, 0)
}
