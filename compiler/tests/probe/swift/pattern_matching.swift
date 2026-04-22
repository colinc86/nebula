// switch on enum with associated values
enum Shape {
    case circle(r: Double)
    case square(side: Double)
    case rect(w: Double, h: Double)
}

func area(_ s: Shape) -> Double {
    switch s {
    case .circle(let r):
        return 3.14159 * r * r
    case .square(let side):
        return side * side
    case .rect(let w, let h):
        return w * h
    }
}

// tuple pattern match with where
func classify(_ p: (Int, Int)) -> String {
    switch p {
    case (0, 0): return "origin"
    case (_, 0): return "x-axis"
    case (0, _): return "y-axis"
    case let (x, y) where x == y: return "diagonal"
    default: return "other"
    }
}

// if case let
func unwrapCircle(_ s: Shape) -> Double? {
    if case .circle(let r) = s { return r }
    return nil
}
