// Task Manager — Benchmark corpus (C)

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_TASKS 100
#define MAX_TITLE 128

typedef struct {
    int id;
    char title[MAX_TITLE];
    int priority;
    int done;
} Task;

typedef struct {
    Task tasks[MAX_TASKS];
    int count;
    int next_id;
} TaskManager;

void task_summary(Task *task, char *buf, int bufsize) {
    const char *status = task->done ? "done" : "pending";
    snprintf(buf, bufsize, "[%d] %s (pri=%d, %s)",
             task->id, task->title, task->priority, status);
}

void task_complete(Task *task) {
    task->done = 1;
}

void manager_init(TaskManager *m) {
    m->count = 0;
    m->next_id = 1;
}

Task *manager_add(TaskManager *m, const char *title, int priority) {
    if (m->count >= MAX_TASKS) return NULL;
    Task *task = &m->tasks[m->count];
    task->id = m->next_id;
    strncpy(task->title, title, MAX_TITLE - 1);
    task->title[MAX_TITLE - 1] = '\0';
    task->priority = priority;
    task->done = 0;
    m->count++;
    m->next_id++;
    return task;
}

void manager_remove(TaskManager *m, int id) {
    for (int i = 0; i < m->count; i++) {
        if (m->tasks[i].id == id) {
            for (int j = i; j < m->count - 1; j++) {
                m->tasks[j] = m->tasks[j + 1];
            }
            m->count--;
            return;
        }
    }
}

Task *manager_find(TaskManager *m, int id) {
    for (int i = 0; i < m->count; i++) {
        if (m->tasks[i].id == id) {
            return &m->tasks[i];
        }
    }
    return NULL;
}

int manager_pending_count(TaskManager *m) {
    int count = 0;
    for (int i = 0; i < m->count; i++) {
        if (!m->tasks[i].done) {
            count++;
        }
    }
    return count;
}

int manager_completed_count(TaskManager *m) {
    int count = 0;
    for (int i = 0; i < m->count; i++) {
        if (m->tasks[i].done) {
            count++;
        }
    }
    return count;
}

const char *priority_label(int p) {
    if (p == 1) return "critical";
    if (p == 2) return "high";
    if (p == 3) return "medium";
    if (p == 4) return "low";
    return "unknown";
}

int batch_complete(TaskManager *m, int *ids, int id_count) {
    int count = 0;
    for (int i = 0; i < id_count; i++) {
        Task *task = manager_find(m, ids[i]);
        if (task != NULL && !task->done) {
            task_complete(task);
            count++;
        }
    }
    return count;
}

void print_report(TaskManager *m) {
    printf("=== Task Report ===\n");
    int total = m->count;
    int done = manager_completed_count(m);
    int pend = total - done;
    double pct = total == 0 ? 0.0 : (double)done / (double)total * 100.0;
    printf("Total: %d, Done: %d, Pending: %d\n", total, done, pend);
    printf("Completion: %.1f%%\n", pct);
    printf("\n");

    char buf[256];
    for (int i = 0; i < m->count; i++) {
        task_summary(&m->tasks[i], buf, sizeof(buf));
        const char *label = priority_label(m->tasks[i].priority);
        printf("  %s [%s]\n", buf, label);
    }
}

int fibonacci(int n) {
    if (n <= 1) return n;
    int a = 0;
    int b = 1;
    for (int i = 2; i <= n; i++) {
        int temp = b;
        b = a + b;
        a = temp;
    }
    return b;
}

int process_numbers(int *numbers, int len) {
    int total = 0;
    for (int i = 0; i < len; i++) {
        int doubled = numbers[i] * 2;
        if (doubled % 2 == 0) {
            total += doubled;
        }
    }
    return total;
}

int main() {
    TaskManager manager;
    manager_init(&manager);
    manager_add(&manager, "Design database schema", 1);
    manager_add(&manager, "Write API endpoints", 2);
    manager_add(&manager, "Add authentication", 2);
    manager_add(&manager, "Write unit tests", 3);
    manager_add(&manager, "Deploy to staging", 4);

    int ids[] = {1, 3};
    batch_complete(&manager, ids, 2);
    print_report(&manager);

    int result = fibonacci(10);
    printf("Fibonacci(10) = %d\n", result);

    int nums[] = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};
    int total = process_numbers(nums, 10);
    printf("Processed total: %d\n", total);

    return 0;
}
