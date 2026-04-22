fn upto(n: i32) -> i32 {
    (0..n).sum()
}

fn indexed(xs: &[i32]) -> Vec<(usize, i32)> {
    xs.iter().enumerate().map(|(i, &x)| (i, x)).collect()
}

fn zipped(a: &[i32], b: &[i32]) -> Vec<(i32, i32)> {
    a.iter().zip(b.iter()).map(|(&x, &y)| (x, y)).collect()
}

fn stepped(start: i32, stop: i32, step: usize) -> Vec<i32> {
    (start..stop).step_by(step).collect()
}

// nested loop with labels
fn first_pair_sum(xs: &[i32], target: i32) -> Option<(i32, i32)> {
    for (i, &a) in xs.iter().enumerate() {
        for &b in &xs[i + 1..] {
            if a + b == target {
                return Some((a, b));
            }
        }
    }
    None
}

// while let
fn drain_stack(v: &mut Vec<i32>) -> i32 {
    let mut total = 0;
    while let Some(x) = v.pop() {
        total += x;
    }
    total
}
