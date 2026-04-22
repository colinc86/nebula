protocol Speakable {
    func speak() -> String
}

struct Dog: Speakable {
    let name: String
    func speak() -> String {
        return "\(name) barks"
    }
}

// optional unwrap
func greet(_ name: String?) -> String {
    guard let n = name else { return "anonymous" }
    return "hi, \(n)"
}

// computed property
struct Temperature {
    var celsius: Double
    var fahrenheit: Double {
        get { return celsius * 9 / 5 + 32 }
        set { celsius = (newValue - 32) * 5 / 9 }
    }
}

// closure with trailing syntax
func apply(to xs: [Int], _ f: (Int) -> Int) -> [Int] {
    return xs.map(f)
}

// enum with associated values
enum Result<T, E> {
    case success(T)
    case failure(E)
}
