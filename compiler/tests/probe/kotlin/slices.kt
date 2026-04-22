fun firstThree(xs: List<Int>): List<Int> = xs.take(3)

fun tail(xs: List<Int>): List<Int> = xs.drop(1)

fun lastOrNull(xs: List<Int>): Int? = xs.lastOrNull()

fun middle(xs: List<Int>, lo: Int, hi: Int): List<Int> = xs.subList(lo, hi)

fun prefixStr(s: String, n: Int): String = s.take(n)

// range
val r1 = 0 until 10
val r2 = 0..10
val r3 = 10 downTo 0 step 2
