// Task Manager — Benchmark corpus (JavaScript)

class Task {
    constructor(id, title, priority, done) {
        this.id = id;
        this.title = title;
        this.priority = priority;
        this.done = done;
    }

    summary() {
        const status = this.done ? "done" : "pending";
        return `[${this.id}] ${this.title} (pri=${this.priority}, ${status})`;
    }

    complete() {
        this.done = true;
    }
}

class TaskManager {
    constructor() {
        this.tasks = [];
        this.nextId = 1;
    }

    add(title, priority = 3) {
        const task = new Task(this.nextId, title, priority, false);
        this.tasks.push(task);
        this.nextId += 1;
        return task;
    }

    remove(id) {
        this.tasks = this.tasks.filter(t => t.id !== id);
    }

    find(id) {
        for (const task of this.tasks) {
            if (task.id === id) {
                return task;
            }
        }
        return null;
    }

    pending() {
        return this.tasks.filter(t => !t.done);
    }

    completed() {
        return this.tasks.filter(t => t.done);
    }

    byPriority() {
        return [...this.tasks].sort((a, b) => a.priority - b.priority);
    }

    stats() {
        const total = this.tasks.length;
        const done = this.completed().length;
        const pend = total - done;
        const pct = total === 0 ? 0.0 : (done / total) * 100.0;
        return { total, done, pending: pend, pct };
    }
}

function priorityLabel(p) {
    if (p === 1) {
        return "critical";
    } else if (p === 2) {
        return "high";
    } else if (p === 3) {
        return "medium";
    } else if (p === 4) {
        return "low";
    } else {
        return "unknown";
    }
}

function batchComplete(manager, ids) {
    let count = 0;
    for (const id of ids) {
        const task = manager.find(id);
        if (task !== null && !task.done) {
            task.complete();
            count += 1;
        }
    }
    return count;
}

function printReport(manager) {
    console.log("=== Task Report ===");
    const stats = manager.stats();
    console.log(`Total: ${stats.total}, Done: ${stats.done}, Pending: ${stats.pending}`);
    console.log(`Completion: ${stats.pct.toFixed(1)}%`);
    console.log("");

    const sortedTasks = manager.byPriority();
    for (const task of sortedTasks) {
        const label = priorityLabel(task.priority);
        console.log(`  ${task.summary()} [${label}]`);
    }
}

function fibonacci(n) {
    if (n <= 1) {
        return n;
    }
    let a = 0;
    let b = 1;
    for (let i = 2; i <= n; i++) {
        const temp = b;
        b = a + b;
        a = temp;
    }
    return b;
}

function processNumbers(numbers) {
    const doubled = numbers.map(x => x * 2);
    const evens = doubled.filter(x => x % 2 === 0);
    let total = 0;
    for (const n of evens) {
        total += n;
    }
    return total;
}

function main() {
    const manager = new TaskManager();
    manager.add("Design database schema", 1);
    manager.add("Write API endpoints", 2);
    manager.add("Add authentication", 2);
    manager.add("Write unit tests", 3);
    manager.add("Deploy to staging", 4);

    try {
        batchComplete(manager, [1, 3]);
        printReport(manager);
    } catch (e) {
        console.log(`Error: ${e.message}`);
    }

    const result = fibonacci(10);
    console.log(`Fibonacci(10) = ${result}`);

    const nums = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    const total = processNumbers(nums);
    console.log(`Processed total: ${total}`);
}

main();
