// named closure
let addOne: (Int) -> Int = { x in x + 1 }

// trailing closure with $0
func doubled(_ xs: [Int]) -> [Int] {
    return xs.map { $0 * 2 }
}

// capture list
func makeCounter() -> () -> Int {
    var n = 0
    return { n += 1; return n }
}

// closure capture [weak self] equivalent simplified
func times(_ n: Int, _ fn: (Int) -> Void) {
    for i in 0..<n { fn(i) }
}

// chained
let result = [1, 2, 3].map { $0 * 2 }.filter { $0 > 2 }.reduce(0, +)
