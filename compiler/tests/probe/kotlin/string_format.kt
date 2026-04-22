fun greet(name: String, age: Int): String = "hi $name, you are $age"

fun fmtFloat(x: Double): String = "%.2f".format(x)

fun describe(a: Int, b: Int): String = "sum=${a + b} prod=${a * b}"

// multi-line raw string
fun banner(title: String): String = """
    === $title ===
    ok
""".trimIndent()

fun joined(xs: List<String>): String = xs.joinToString(prefix = "[", postfix = "]", separator = ", ")
