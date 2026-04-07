// Task Manager — Benchmark corpus (Kotlin)

data class Task(
    val id: Int,
    val title: String,
    val priority: Int,
    var done: Boolean
) {
    fun summary(): String {
        val status = if (done) "done" else "pending"
        return "[$id] $title (pri=$priority, $status)"
    }

    fun complete() {
        done = true
    }
}

class TaskManager {
    val tasks = mutableListOf<Task>()
    var nextId = 1

    fun add(title: String, priority: Int = 3): Task {
        val task = Task(nextId, title, priority, false)
        tasks.add(task)
        nextId += 1
        return task
    }

    fun remove(id: Int) {
        tasks.removeAll { it.id == id }
    }

    fun find(id: Int): Task? {
        for (task in tasks) {
            if (task.id == id) {
                return task
            }
        }
        return null
    }

    fun pending(): List<Task> {
        return tasks.filter { !it.done }
    }

    fun completed(): List<Task> {
        return tasks.filter { it.done }
    }

    fun byPriority(): List<Task> {
        return tasks.sortedBy { it.priority }
    }

    fun stats(): Map<String, Any> {
        val total = tasks.size
        val done = completed().size
        val pend = total - done
        val pct = if (total == 0) 0.0 else done.toDouble() / total.toDouble() * 100.0
        return mapOf("total" to total, "done" to done, "pending" to pend, "pct" to pct)
    }
}

fun priorityLabel(p: Int): String {
    return when (p) {
        1 -> "critical"
        2 -> "high"
        3 -> "medium"
        4 -> "low"
        else -> "unknown"
    }
}

fun batchComplete(manager: TaskManager, ids: List<Int>): Int {
    var count = 0
    for (id in ids) {
        val task = manager.find(id)
        if (task != null && !task.done) {
            task.complete()
            count += 1
        }
    }
    return count
}

fun printReport(manager: TaskManager) {
    println("=== Task Report ===")
    val stats = manager.stats()
    println("Total: ${stats["total"]}, Done: ${stats["done"]}, Pending: ${stats["pending"]}")
    println("Completion: ${"%.1f".format(stats["pct"])}%")
    println("")

    val sortedTasks = manager.byPriority()
    for (task in sortedTasks) {
        val label = priorityLabel(task.priority)
        println("  ${task.summary()} [$label]")
    }
}

fun fibonacci(n: Int): Int {
    if (n <= 1) return n
    var a = 0
    var b = 1
    for (i in 2..n) {
        val temp = b
        b = a + b
        a = temp
    }
    return b
}

fun processNumbers(numbers: List<Int>): Int {
    val doubled = numbers.map { it * 2 }
    val evens = doubled.filter { it % 2 == 0 }
    var total = 0
    for (n in evens) {
        total += n
    }
    return total
}

fun main() {
    val manager = TaskManager()
    manager.add("Design database schema", 1)
    manager.add("Write API endpoints", 2)
    manager.add("Add authentication", 2)
    manager.add("Write unit tests", 3)
    manager.add("Deploy to staging", 4)

    try {
        batchComplete(manager, listOf(1, 3))
        printReport(manager)
    } catch (e: Exception) {
        println("Error: ${e.message}")
    }

    val result = fibonacci(10)
    println("Fibonacci(10) = $result")

    val nums = listOf(1, 2, 3, 4, 5, 6, 7, 8, 9, 10)
    val total = processNumbers(nums)
    println("Processed total: $total")
}
