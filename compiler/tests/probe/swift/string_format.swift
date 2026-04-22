func greet(_ name: String, _ age: Int) -> String {
    return "hi \(name), you are \(age)"
}

func fmtFloat(_ x: Double) -> String {
    return String(format: "%.2f", x)
}

// multiline string
func banner(_ title: String) -> String {
    return """
    === \(title) ===
    ok
    """
}

// expression interpolation
func describe(_ a: Int, _ b: Int) -> String {
    return "sum=\(a + b) prod=\(a * b)"
}
