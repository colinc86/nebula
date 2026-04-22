func firstThree(_ xs: [Int]) -> [Int] {
    return Array(xs.prefix(3))
}

func tail(_ xs: [Int]) -> [Int] {
    return Array(xs.dropFirst())
}

func last(_ xs: [Int]) -> Int? {
    return xs.last
}

func middle(_ xs: [Int], lo: Int, hi: Int) -> [Int] {
    return Array(xs[lo..<hi])
}

func prefixStr(_ s: String, _ n: Int) -> String {
    return String(s.prefix(n))
}

// range expressions
let r1 = 0..<10
let r2 = 0...10
