func upto(_ n: Int) -> Int {
    var total = 0
    for i in 0..<n { total += i }
    return total
}

func indexed<T>(_ xs: [T]) -> [(Int, T)] {
    return Array(xs.enumerated())
}

func zipped<A, B>(_ a: [A], _ b: [B]) -> [(A, B)] {
    return Array(zip(a, b))
}

// stride
func stepped(_ start: Int, _ stop: Int, _ step: Int) -> [Int] {
    return Array(stride(from: start, to: stop, by: step))
}

// while loop
func countDown(_ n: Int) -> Int {
    var x = n
    var steps = 0
    while x > 0 { x -= 1; steps += 1 }
    return steps
}
