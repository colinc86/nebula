// data class
data class User(val id: Int, val name: String, val email: String? = null)

// sealed class + when with exhaustive match
sealed class Shape {
    data class Circle(val r: Double) : Shape()
    data class Square(val side: Double) : Shape()
}

fun area(s: Shape): Double = when (s) {
    is Shape.Circle -> 3.14159 * s.r * s.r
    is Shape.Square -> s.side * s.side
}

// extension function
fun String.shout(): String = this.uppercase() + "!"

// higher-order function with lambda
fun <T, R> List<T>.mapIndexedTo(f: (Int, T) -> R): List<R> {
    val out = mutableListOf<R>()
    for ((i, v) in this.withIndex()) {
        out.add(f(i, v))
    }
    return out
}

// null safety / elvis
fun greet(name: String?): String {
    val n = name ?: "anonymous"
    return "hi, $n"
}

// companion object
class Counter {
    companion object {
        var count: Int = 0
        fun inc() { count += 1 }
    }
}
