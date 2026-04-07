// Task Manager — Benchmark corpus (TypeScript)

interface TaskData {
    id: number;
    title: string;
    priority: number;
    done: boolean;
}

interface Stats {
    total: number;
    done: number;
    pending: number;
    pct: number;
}

class Task {
    id: number;
    title: string;
    priority: number;
    done: boolean;

    constructor(id: number, title: string, priority: number, done: boolean) {
        this.id = id;
        this.title = title;
        this.priority = priority;
        this.done = done;
    }

    summary(): string {
        const status: string = this.done ? "done" : "pending";
        return `[${this.id}] ${this.title} (pri=${this.priority}, ${status})`;
    }

    complete(): void {
        this.done = true;
    }
}

class TaskManager {
    tasks: Task[];
    nextId: number;

    constructor() {
        this.tasks = [];
        this.nextId = 1;
    }

    add(title: string, priority: number = 3): Task {
        const task = new Task(this.nextId, title, priority, false);
        this.tasks.push(task);
        this.nextId += 1;
        return task;
    }

    remove(id: number): void {
        this.tasks = this.tasks.filter((t: Task) => t.id !== id);
    }

    find(id: number): Task | null {
        for (const task of this.tasks) {
            if (task.id === id) {
                return task;
            }
        }
        return null;
    }

    pending(): Task[] {
        return this.tasks.filter((t: Task) => !t.done);
    }

    completed(): Task[] {
        return this.tasks.filter((t: Task) => t.done);
    }

    byPriority(): Task[] {
        return [...this.tasks].sort((a: Task, b: Task) => a.priority - b.priority);
    }

    stats(): Stats {
        const total: number = this.tasks.length;
        const done: number = this.completed().length;
        const pend: number = total - done;
        const pct: number = total === 0 ? 0.0 : (done / total) * 100.0;
        return { total, done, pending: pend, pct };
    }
}

function priorityLabel(p: number): string {
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

function batchComplete(manager: TaskManager, ids: number[]): number {
    let count: number = 0;
    for (const id of ids) {
        const task: Task | null = manager.find(id);
        if (task !== null && !task.done) {
            task.complete();
            count += 1;
        }
    }
    return count;
}

function printReport(manager: TaskManager): void {
    console.log("=== Task Report ===");
    const stats: Stats = manager.stats();
    console.log(`Total: ${stats.total}, Done: ${stats.done}, Pending: ${stats.pending}`);
    console.log(`Completion: ${stats.pct.toFixed(1)}%`);
    console.log("");

    const sortedTasks: Task[] = manager.byPriority();
    for (const task of sortedTasks) {
        const label: string = priorityLabel(task.priority);
        console.log(`  ${task.summary()} [${label}]`);
    }
}

function fibonacci(n: number): number {
    if (n <= 1) {
        return n;
    }
    let a: number = 0;
    let b: number = 1;
    for (let i: number = 2; i <= n; i++) {
        const temp: number = b;
        b = a + b;
        a = temp;
    }
    return b;
}

function processNumbers(numbers: number[]): number {
    const doubled: number[] = numbers.map((x: number) => x * 2);
    const evens: number[] = doubled.filter((x: number) => x % 2 === 0);
    let total: number = 0;
    for (const n of evens) {
        total += n;
    }
    return total;
}

function main(): void {
    const manager: TaskManager = new TaskManager();
    manager.add("Design database schema", 1);
    manager.add("Write API endpoints", 2);
    manager.add("Add authentication", 2);
    manager.add("Write unit tests", 3);
    manager.add("Deploy to staging", 4);

    try {
        batchComplete(manager, [1, 3]);
        printReport(manager);
    } catch (e) {
        console.log(`Error: ${(e as Error).message}`);
    }

    const result: number = fibonacci(10);
    console.log(`Fibonacci(10) = ${result}`);

    const nums: number[] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    const total: number = processNumbers(nums);
    console.log(`Processed total: ${total}`);
}

main();
