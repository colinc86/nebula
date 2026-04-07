// Task Manager — Benchmark corpus (Go)

package main

import "fmt"

type Task struct {
	ID       int
	Title    string
	Priority int
	Done     bool
}

func (t *Task) Summary() string {
	status := "pending"
	if t.Done {
		status = "done"
	}
	return fmt.Sprintf("[%d] %s (pri=%d, %s)", t.ID, t.Title, t.Priority, status)
}

func (t *Task) Complete() {
	t.Done = true
}

type TaskManager struct {
	Tasks  []Task
	NextID int
}

func NewTaskManager() *TaskManager {
	return &TaskManager{Tasks: []Task{}, NextID: 1}
}

func (m *TaskManager) Add(title string, priority int) *Task {
	task := Task{ID: m.NextID, Title: title, Priority: priority, Done: false}
	m.Tasks = append(m.Tasks, task)
	m.NextID++
	return &m.Tasks[len(m.Tasks)-1]
}

func (m *TaskManager) Remove(id int) {
	filtered := []Task{}
	for _, t := range m.Tasks {
		if t.ID != id {
			filtered = append(filtered, t)
		}
	}
	m.Tasks = filtered
}

func (m *TaskManager) Find(id int) *Task {
	for i := range m.Tasks {
		if m.Tasks[i].ID == id {
			return &m.Tasks[i]
		}
	}
	return nil
}

func (m *TaskManager) Pending() []Task {
	result := []Task{}
	for _, t := range m.Tasks {
		if !t.Done {
			result = append(result, t)
		}
	}
	return result
}

func (m *TaskManager) Completed() []Task {
	result := []Task{}
	for _, t := range m.Tasks {
		if t.Done {
			result = append(result, t)
		}
	}
	return result
}

func (m *TaskManager) Stats() map[string]interface{} {
	total := len(m.Tasks)
	done := len(m.Completed())
	pend := total - done
	pct := 0.0
	if total > 0 {
		pct = float64(done) / float64(total) * 100.0
	}
	return map[string]interface{}{
		"total":   total,
		"done":    done,
		"pending": pend,
		"pct":     pct,
	}
}

func PriorityLabel(p int) string {
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

func BatchComplete(manager *TaskManager, ids []int) int {
	count := 0
	for _, id := range ids {
		task := manager.Find(id)
		if task != nil && !task.Done {
			task.Complete()
			count++
		}
	}
	return count
}

func PrintReport(manager *TaskManager) {
	fmt.Println("=== Task Report ===")
	stats := manager.Stats()
	fmt.Printf("Total: %v, Done: %v, Pending: %v\n", stats["total"], stats["done"], stats["pending"])
	fmt.Printf("Completion: %.1f%%\n", stats["pct"])
	fmt.Println("")

	for _, task := range manager.Tasks {
		label := PriorityLabel(task.Priority)
		fmt.Printf("  %s [%s]\n", task.Summary(), label)
	}
}

func Fibonacci(n int) int {
	if n <= 1 {
		return n
	}
	a, b := 0, 1
	for i := 2; i <= n; i++ {
		a, b = b, a+b
	}
	return b
}

func ProcessNumbers(numbers []int) int {
	total := 0
	for _, n := range numbers {
		doubled := n * 2
		if doubled%2 == 0 {
			total += doubled
		}
	}
	return total
}

func main() {
	manager := NewTaskManager()
	manager.Add("Design database schema", 1)
	manager.Add("Write API endpoints", 2)
	manager.Add("Add authentication", 2)
	manager.Add("Write unit tests", 3)
	manager.Add("Deploy to staging", 4)

	BatchComplete(manager, []int{1, 3})
	PrintReport(manager)

	result := Fibonacci(10)
	fmt.Printf("Fibonacci(10) = %d\n", result)

	nums := []int{1, 2, 3, 4, 5, 6, 7, 8, 9, 10}
	total := ProcessNumbers(nums)
	fmt.Printf("Processed total: %d\n", total)
}
