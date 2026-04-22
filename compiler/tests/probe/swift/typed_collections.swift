func sumInts(_ xs: [Int]) -> Int {
    return xs.reduce(0, +)
}

func lookup(_ d: [String: Int], _ k: String) -> Int? {
    return d[k]
}

func toSet(_ xs: [String]) -> Set<String> {
    return Set(xs)
}

func makeIndex(_ words: Array<String>) -> Dictionary<String, Int> {
    var out: Dictionary<String, Int> = [:]
    for (i, w) in words.enumerated() { out[w] = i }
    return out
}

// nested
func grid(_ n: Int) -> [[Int]] {
    return Array(repeating: Array(repeating: 0, count: n), count: n)
}
