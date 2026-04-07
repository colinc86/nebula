// Task Manager — Benchmark corpus (Rust)

struct Task {
    id: i32,
    title: String,
    priority: i32,
    done: bool,
}

impl Task {
    fn new(id: i32, title: &str, priority: i32) -> Task {
        Task {
            id,
            title: title.to_string(),
            priority,
            done: false,
        }
    }

    fn summary(&self) -> String {
        let status = if self.done { "done" } else { "pending" };
        format!("[{}] {} (pri={}, {})", self.id, self.title, self.priority, status)
    }

    fn complete(&mut self) {
        self.done = true;
    }
}

struct TaskManager {
    tasks: Vec<Task>,
    next_id: i32,
}

impl TaskManager {
    fn new() -> TaskManager {
        TaskManager {
            tasks: Vec::new(),
            next_id: 1,
        }
    }

    fn add(&mut self, title: &str, priority: i32) {
        let task = Task::new(self.next_id, title, priority);
        self.tasks.push(task);
        self.next_id += 1;
    }

    fn remove(&mut self, id: i32) {
        self.tasks.retain(|t| t.id != id);
    }

    fn find(&mut self, id: i32) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    fn pending(&self) -> Vec<&Task> {
        self.tasks.iter().filter(|t| !t.done).collect()
    }

    fn completed(&self) -> Vec<&Task> {
        self.tasks.iter().filter(|t| t.done).collect()
    }

    fn by_priority(&self) -> Vec<&Task> {
        let mut sorted: Vec<&Task> = self.tasks.iter().collect();
        sorted.sort_by_key(|t| t.priority);
        sorted
    }

    fn stats(&self) -> (usize, usize, usize, f64) {
        let total = self.tasks.len();
        let done = self.completed().len();
        let pend = total - done;
        let pct = if total == 0 { 0.0 } else { done as f64 / total as f64 * 100.0 };
        (total, done, pend, pct)
    }
}

fn priority_label(p: i32) -> &'static str {
    match p {
        1 => "critical",
        2 => "high",
        3 => "medium",
        4 => "low",
        _ => "unknown",
    }
}

fn batch_complete(manager: &mut TaskManager, ids: &[i32]) -> i32 {
    let mut count = 0;
    for &id in ids {
        if let Some(task) = manager.find(id) {
            if !task.done {
                task.complete();
                count += 1;
            }
        }
    }
    count
}

fn print_report(manager: &TaskManager) {
    println!("=== Task Report ===");
    let (total, done, pend, pct) = manager.stats();
    println!("Total: {}, Done: {}, Pending: {}", total, done, pend);
    println!("Completion: {:.1}%", pct);
    println!();

    let sorted_tasks = manager.by_priority();
    for task in sorted_tasks {
        let label = priority_label(task.priority);
        println!("  {} [{}]", task.summary(), label);
    }
}

fn fibonacci(n: i32) -> i32 {
    if n <= 1 {
        return n;
    }
    let mut a = 0;
    let mut b = 1;
    for _ in 2..=n {
        let temp = b;
        b = a + b;
        a = temp;
    }
    b
}

fn process_numbers(numbers: &[i32]) -> i32 {
    let doubled: Vec<i32> = numbers.iter().map(|x| x * 2).collect();
    let evens: Vec<i32> = doubled.iter().filter(|x| *x % 2 == 0).copied().collect();
    let mut total = 0;
    for n in evens {
        total += n;
    }
    total
}

fn main() {
    let mut manager = TaskManager::new();
    manager.add("Design database schema", 1);
    manager.add("Write API endpoints", 2);
    manager.add("Add authentication", 2);
    manager.add("Write unit tests", 3);
    manager.add("Deploy to staging", 4);

    batch_complete(&mut manager, &[1, 3]);
    print_report(&manager);

    let result = fibonacci(10);
    println!("Fibonacci(10) = {}", result);

    let nums = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let total = process_numbers(&nums);
    println!("Processed total: {}", total);
}
