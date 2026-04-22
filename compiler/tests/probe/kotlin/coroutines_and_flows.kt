import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*

// suspend function
suspend fun fetchUser(id: Int): String {
    delay(100)
    return "user_$id"
}

// coroutine scope with async
suspend fun fetchAll(ids: List<Int>): List<String> = coroutineScope {
    ids.map { async { fetchUser(it) } }.map { it.await() }
}

// flow builder
fun counter(start: Int): Flow<Int> = flow {
    var n = start
    while (n < start + 5) {
        emit(n)
        n += 1
    }
}

// try / catch / finally
fun safeDivide(a: Int, b: Int): Int? {
    return try {
        a / b
    } catch (e: ArithmeticException) {
        null
    } finally {
        println("done")
    }
}
