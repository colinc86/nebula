fn first_three(xs: &[i32]) -> &[i32] {
    &xs[..3]
}

fn tail(xs: &[i32]) -> &[i32] {
    &xs[1..]
}

fn middle(xs: &[i32], lo: usize, hi: usize) -> &[i32] {
    &xs[lo..hi]
}

fn last_copy(xs: &[i32]) -> Option<i32> {
    xs.last().copied()
}

fn prefix(s: &str, n: usize) -> &str {
    &s[..n]
}

// range and step
fn every_other(xs: &[i32]) -> Vec<i32> {
    xs.iter().step_by(2).copied().collect()
}
