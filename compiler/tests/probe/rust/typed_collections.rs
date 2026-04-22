use std::collections::{HashMap, HashSet, BTreeMap};

fn sum_ints(xs: &[i32]) -> i32 {
    xs.iter().sum()
}

fn lookup<'a>(d: &'a HashMap<String, i32>, k: &str) -> Option<&'a i32> {
    d.get(k)
}

fn to_set(xs: &[String]) -> HashSet<String> {
    xs.iter().cloned().collect()
}

fn make_index(words: Vec<String>) -> BTreeMap<String, usize> {
    words.into_iter().enumerate().map(|(i, w)| (w, i)).collect()
}

fn grid(n: usize) -> Vec<Vec<i32>> {
    vec![vec![0; n]; n]
}
