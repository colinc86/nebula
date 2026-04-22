enum class Color(val css: String) {
    RED("red"),
    GREEN("green"),
    BLUE("blue");

    fun bright(): String = css.uppercase()
}

enum class Level {
    DEBUG, INFO, WARN, ERROR;

    fun isSerious(): Boolean = this == WARN || this == ERROR
}

fun label(c: Color): String = when (c) {
    Color.RED -> "r"
    Color.GREEN -> "g"
    Color.BLUE -> "b"
}
