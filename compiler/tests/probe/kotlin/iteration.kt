fun upto(n: Int): Int {
    var total = 0
    for (i in 0 until n) total += i
    return total
}

fun indexed(xs: List<Int>): List<Pair<Int, Int>> {
    val out = mutableListOf<Pair<Int, Int>>()
    for ((i, x) in xs.withIndex()) out.add(i to x)
    return out
}

fun zipped(a: List<Int>, b: List<Int>): List<Pair<Int, Int>> = a.zip(b)

fun stepped(start: Int, stop: Int, step: Int): List<Int> = (start until stop step step).toList()

fun countDown(n: Int): Int {
    var x = n
    var steps = 0
    while (x > 0) { x -= 1; steps += 1 }
    return steps
}
