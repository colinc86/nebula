// tuple return
func divmod(_ a: Int, _ b: Int) -> (Int, Int) {
    return (a / b, a % b)
}

// labeled tuple
func bounds(_ xs: [Int]) -> (lo: Int, hi: Int) {
    return (xs.min() ?? 0, xs.max() ?? 0)
}

// destructuring
func callDivmod() -> Int {
    let (q, r) = divmod(10, 3)
    return q + r
}

// tuple as return with named fields
func pair(_ a: Int, _ b: String) -> (num: Int, str: String) {
    return (num: a, str: b)
}
