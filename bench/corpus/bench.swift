// Task Manager — Benchmark corpus (Swift)

struct Task {
    let id: Int
    let title: String
    let priority: Int
    var done: Bool

    func summary() -> String {
        let status = done ? "done" : "pending"
        return "[\(id)] \(title) (pri=\(priority), \(status))"
    }

    mutating func complete() {
        done = true
    }
}

class TaskManager {
    var tasks: [Task] = []
    var nextId: Int = 1

    func add(title: String, priority: Int = 3) -> Task {
        let task = Task(id: nextId, title: title, priority: priority, done: false)
        tasks.append(task)
        nextId += 1
        return task
    }

    func remove(id: Int) {
        tasks = tasks.filter { $0.id != id }
    }

    func find(id: Int) -> Task? {
        for task in tasks {
            if task.id == id {
                return task
            }
        }
        return nil
    }

    func pending() -> [Task] {
        return tasks.filter { !$0.done }
    }

    func completed() -> [Task] {
        return tasks.filter { $0.done }
    }

    func byPriority() -> [Task] {
        return tasks.sorted { $0.priority < $1.priority }
    }

    func stats() -> [String: Any] {
        let total = tasks.count
        let done = completed().count
        let pend = total - done
        let pct = total == 0 ? 0.0 : Double(done) / Double(total) * 100.0
        return ["total": total, "done": done, "pending": pend, "pct": pct]
    }
}

func priorityLabel(_ p: Int) -> String {
    if p == 1 {
        return "critical"
    } else if p == 2 {
        return "high"
    } else if p == 3 {
        return "medium"
    } else if p == 4 {
        return "low"
    } else {
        return "unknown"
    }
}

func batchComplete(_ manager: TaskManager, ids: [Int]) -> Int {
    var count = 0
    for id in ids {
        if var task = manager.find(id: id) {
            if !task.done {
                task.complete()
                count += 1
            }
        }
    }
    return count
}

func printReport(_ manager: TaskManager) {
    print("=== Task Report ===")
    let stats = manager.stats()
    print("Total: \(stats["total"]!), Done: \(stats["done"]!), Pending: \(stats["pending"]!)")
    print("Completion: \(String(format: "%.1f", stats["pct"] as! Double))%")
    print("")

    let sortedTasks = manager.byPriority()
    for task in sortedTasks {
        let label = priorityLabel(task.priority)
        print("  \(task.summary()) [\(label)]")
    }
}

func fibonacci(_ n: Int) -> Int {
    if n <= 1 {
        return n
    }
    var a = 0
    var b = 1
    for _ in 2...n {
        let temp = b
        b = a + b
        a = temp
    }
    return b
}

func processNumbers(_ numbers: [Int]) -> Int {
    let doubled = numbers.map { $0 * 2 }
    let evens = doubled.filter { $0 % 2 == 0 }
    var total = 0
    for n in evens {
        total += n
    }
    return total
}

func main() {
    let manager = TaskManager()
    manager.add(title: "Design database schema", priority: 1)
    manager.add(title: "Write API endpoints", priority: 2)
    manager.add(title: "Add authentication", priority: 2)
    manager.add(title: "Write unit tests", priority: 3)
    manager.add(title: "Deploy to staging", priority: 4)

    batchComplete(manager, ids: [1, 3])
    printReport(manager)

    let result = fibonacci(10)
    print("Fibonacci(10) = \(result)")

    let nums = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    let total = processNumbers(nums)
    print("Processed total: \(total)")
}

main()
