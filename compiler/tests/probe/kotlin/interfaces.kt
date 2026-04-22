interface Speakable {
    fun speak(): String
    val name: String
}

class Dog(override val name: String) : Speakable {
    override fun speak(): String = "$name barks"
}

// interface with default
interface Greeter {
    fun greet(name: String): String = "hi, $name"
}

class Polite : Greeter {
    override fun greet(name: String): String = "hello, $name"
}

// abstract class
abstract class Shape {
    abstract fun area(): Double
    open fun describe(): String = "shape with area ${area()}"
}

class Square(val side: Double) : Shape() {
    override fun area(): Double = side * side
}
