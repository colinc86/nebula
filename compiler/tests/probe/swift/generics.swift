// generic function with protocol constraint
func maxOf<T: Comparable>(_ a: T, _ b: T) -> T {
    return a > b ? a : b
}

// generic container
struct Stack<T> {
    private var items: [T] = []
    mutating func push(_ x: T) { items.append(x) }
    mutating func pop() -> T? { return items.popLast() }
}

// where clause
func firstEven<S: Sequence>(_ xs: S) -> S.Element? where S.Element == Int {
    return xs.first { $0 % 2 == 0 }
}

// generic map
func mapAll<T, U>(_ xs: [T], _ f: (T) -> U) -> [U] {
    return xs.map(f)
}
