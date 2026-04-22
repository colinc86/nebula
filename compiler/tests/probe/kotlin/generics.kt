fun <T : Comparable<T>> maxOf2(a: T, b: T): T = if (a > b) a else b

class Stack<T> {
    private val items = mutableListOf<T>()
    fun push(x: T) { items.add(x) }
    fun pop(): T? = if (items.isNotEmpty()) items.removeLast() else null
}

// reified type
inline fun <reified T> filterIs(xs: List<Any>): List<T> {
    return xs.filterIsInstance<T>()
}

// in/out variance
interface Producer<out T> { fun produce(): T }
interface Consumer<in T> { fun consume(x: T) }

fun <T, R> List<T>.mapTo(f: (T) -> R): List<R> = this.map(f)
