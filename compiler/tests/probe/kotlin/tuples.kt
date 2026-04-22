// Pair / Triple
fun divmod(a: Int, b: Int): Pair<Int, Int> = Pair(a / b, a % b)

fun named(a: Int, b: String): Pair<Int, String> = a to b

// destructure
fun callDivmod(): Int {
    val (q, r) = divmod(10, 3)
    return q + r
}

// Triple
fun coords(): Triple<Int, Int, Int> = Triple(1, 2, 3)

// data class destructure
data class Point(val x: Int, val y: Int)

fun origin(): Point = Point(0, 0)

fun useOrigin(): Int {
    val (x, y) = origin()
    return x + y
}
