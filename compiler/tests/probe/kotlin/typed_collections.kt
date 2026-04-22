fun sumInts(xs: List<Int>): Int = xs.sum()

fun lookup(d: Map<String, Int>, k: String): Int? = d[k]

fun toSet(xs: List<String>): Set<String> = xs.toSet()

fun makeIndex(words: List<String>): Map<String, Int> {
    val out = mutableMapOf<String, Int>()
    words.forEachIndexed { i, w -> out[w] = i }
    return out
}

fun grid(n: Int): Array<IntArray> = Array(n) { IntArray(n) }

fun intArray(xs: IntArray): Int {
    var total = 0
    for (x in xs) total += x
    return total
}
