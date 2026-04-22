import java.util.ArrayList
import java.io.File
import java.time.LocalDate

fun readLines(path: String): List<String> {
    return File(path).readLines()
}

fun today(): LocalDate = LocalDate.now()

fun makeList(): java.util.List<String> {
    return ArrayList()
}

fun <T> firstOfAny(xs: java.util.Collection<T>): T? {
    val it = xs.iterator()
    return if (it.hasNext()) it.next() else null
}
