fun makeAdder(n: Int): (Int) -> Int = { x -> x + n }

fun doubled(xs: List<Int>): List<Int> = xs.map { it * 2 }

fun keepPositive(xs: List<Int>): List<Int> = xs.filter { it > 0 }

fun compose(f: (Int) -> Int, g: (Int) -> Int): (Int) -> Int = { x -> f(g(x)) }

fun makeCounter(): () -> Int {
    var n = 0
    return { ++n; n }
}

// chained
val result = listOf(1, 2, 3).map { it * 2 }.filter { it > 2 }.sum()
