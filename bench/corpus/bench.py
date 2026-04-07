# Task Manager — Benchmark corpus (Python)

class Task:
    def __init__(self, id, title, priority, done):
        self.id = id
        self.title = title
        self.priority = priority
        self.done = done

    def summary(self):
        status = "done" if self.done else "pending"
        return f"[{self.id}] {self.title} (pri={self.priority}, {status})"

    def complete(self):
        self.done = True


class TaskManager:
    def __init__(self):
        self.tasks = []
        self.next_id = 1

    def add(self, title, priority=3):
        task = Task(self.next_id, title, priority, False)
        self.tasks.append(task)
        self.next_id += 1
        return task

    def remove(self, id):
        self.tasks = [t for t in self.tasks if t.id != id]

    def find(self, id):
        for task in self.tasks:
            if task.id == id:
                return task
        return None

    def pending(self):
        return [t for t in self.tasks if not t.done]

    def completed(self):
        return [t for t in self.tasks if t.done]

    def by_priority(self):
        return sorted(self.tasks, key=lambda t: t.priority)

    def stats(self):
        total = len(self.tasks)
        done = len(self.completed())
        pend = total - done
        if total == 0:
            pct = 0.0
        else:
            pct = (done / total) * 100.0
        return {"total": total, "done": done, "pending": pend, "pct": pct}


def priority_label(p):
    if p == 1:
        return "critical"
    elif p == 2:
        return "high"
    elif p == 3:
        return "medium"
    elif p == 4:
        return "low"
    else:
        return "unknown"


def batch_complete(manager, ids):
    count = 0
    for id in ids:
        task = manager.find(id)
        if task is not None and not task.done:
            task.complete()
            count += 1
    return count


def print_report(manager):
    print("=== Task Report ===")
    stats = manager.stats()
    print(f"Total: {stats['total']}, Done: {stats['done']}, Pending: {stats['pending']}")
    print(f"Completion: {stats['pct']:.1f}%")
    print("")

    sorted_tasks = manager.by_priority()
    for task in sorted_tasks:
        label = priority_label(task.priority)
        print(f"  {task.summary()} [{label}]")


def fibonacci(n):
    if n <= 1:
        return n
    a, b = 0, 1
    for _ in range(2, n + 1):
        a, b = b, a + b
    return b


def process_numbers(numbers):
    doubled = list(map(lambda x: x * 2, numbers))
    evens = list(filter(lambda x: x % 2 == 0, doubled))
    total = 0
    for n in evens:
        total += n
    return total


def main():
    manager = TaskManager()
    manager.add("Design database schema", 1)
    manager.add("Write API endpoints", 2)
    manager.add("Add authentication", 2)
    manager.add("Write unit tests", 3)
    manager.add("Deploy to staging", 4)

    try:
        batch_complete(manager, [1, 3])
        print_report(manager)
    except Exception as e:
        print(f"Error: {e}")

    result = fibonacci(10)
    print(f"Fibonacci(10) = {result}")

    nums = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    total = process_numbers(nums)
    print(f"Processed total: {total}")


if __name__ == "__main__":
    main()
