package main

import (
	"fmt"
	"math"
	"sort"
	"strings"
	"sync"
	"time"
)

// --- Domain types ---

type Priority int

const (
	Low Priority = iota
	Medium
	High
	Critical
)

func (p Priority) String() string {
	switch p {
	case Low:
		return "low"
	case Medium:
		return "medium"
	case High:
		return "high"
	case Critical:
		return "critical"
	default:
		return "unknown"
	}
}

type Status int

const (
	Open Status = iota
	InProgress
	Blocked
	Done
)

func (s Status) String() string {
	switch s {
	case Open:
		return "open"
	case InProgress:
		return "in_progress"
	case Blocked:
		return "blocked"
	case Done:
		return "done"
	default:
		return "unknown"
	}
}

type Task struct {
	ID        int
	Title     string
	Priority  Priority
	Status    Status
	Tags      []string
	CreatedAt time.Time
	DueDate   *time.Time
	Subtasks  []int
	Assignee  string
}

// --- Thread-safe task store ---

type TaskStore struct {
	mu     sync.RWMutex
	tasks  map[int]*Task
	nextID int
}

func NewTaskStore() *TaskStore {
	return &TaskStore{
		tasks:  make(map[int]*Task),
		nextID: 1,
	}
}

func (s *TaskStore) Add(title string, priority Priority) *Task {
	s.mu.Lock()
	defer s.mu.Unlock()

	t := &Task{
		ID:        s.nextID,
		Title:     title,
		Priority:  priority,
		Status:    Open,
		Tags:      []string{},
		CreatedAt: time.Now(),
	}
	s.tasks[s.nextID] = t
	s.nextID++
	return t
}

func (s *TaskStore) Get(id int) (*Task, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	t, ok := s.tasks[id]
	return t, ok
}

func (s *TaskStore) Remove(id int) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, ok := s.tasks[id]; !ok {
		return false
	}
	delete(s.tasks, id)
	return true
}

func (s *TaskStore) All() []*Task {
	s.mu.RLock()
	defer s.mu.RUnlock()
	result := make([]*Task, 0, len(s.tasks))
	for _, t := range s.tasks {
		result = append(result, t)
	}
	sort.Slice(result, func(i, j int) bool {
		return result[i].ID < result[j].ID
	})
	return result
}

// --- Filtering ---

type Filter struct {
	Priority  *Priority
	Status    *Status
	Tag       string
	Assignee  string
	Overdue   bool
}

func (s *TaskStore) Search(f Filter) []*Task {
	all := s.All()
	var results []*Task
	now := time.Now()

	for _, t := range all {
		if f.Priority != nil && t.Priority != *f.Priority {
			continue
		}
		if f.Status != nil && t.Status != *f.Status {
			continue
		}
		if f.Tag != "" && !hasTag(t, f.Tag) {
			continue
		}
		if f.Assignee != "" && t.Assignee != f.Assignee {
			continue
		}
		if f.Overdue && (t.DueDate == nil || t.DueDate.After(now) || t.Status == Done) {
			continue
		}
		results = append(results, t)
	}
	return results
}

func hasTag(t *Task, tag string) bool {
	for _, tg := range t.Tags {
		if tg == tag {
			return true
		}
	}
	return false
}

// --- Analytics ---

type Stats struct {
	Total       int
	ByStatus    map[Status]int
	ByPriority  map[Priority]int
	AvgAge      time.Duration
	OverdueCount int
	Unassigned  int
}

func (s *TaskStore) ComputeStats() Stats {
	all := s.All()
	now := time.Now()

	stats := Stats{
		Total:      len(all),
		ByStatus:   make(map[Status]int),
		ByPriority: make(map[Priority]int),
	}

	var totalAge time.Duration
	for _, t := range all {
		stats.ByStatus[t.Status]++
		stats.ByPriority[t.Priority]++
		totalAge += now.Sub(t.CreatedAt)

		if t.DueDate != nil && t.DueDate.Before(now) && t.Status != Done {
			stats.OverdueCount++
		}
		if t.Assignee == "" {
			stats.Unassigned++
		}
	}

	if len(all) > 0 {
		stats.AvgAge = totalAge / time.Duration(len(all))
	}
	return stats
}

// --- Dependency graph (cycle detection) ---

func (s *TaskStore) HasCycle(taskID int) bool {
	visited := make(map[int]bool)
	return s.detectCycle(taskID, visited)
}

func (s *TaskStore) detectCycle(id int, visited map[int]bool) bool {
	if visited[id] {
		return true
	}
	visited[id] = true

	t, ok := s.Get(id)
	if !ok {
		return false
	}
	for _, sub := range t.Subtasks {
		if s.detectCycle(sub, visited) {
			return true
		}
	}

	visited[id] = false
	return false
}

// --- Batch operations ---

func (s *TaskStore) BulkUpdateStatus(ids []int, status Status) (updated int, failed []int) {
	for _, id := range ids {
		s.mu.Lock()
		t, ok := s.tasks[id]
		if ok {
			t.Status = status
			updated++
		} else {
			failed = append(failed, id)
		}
		s.mu.Unlock()
	}
	return
}

func (s *TaskStore) AutoAssign(assignees []string) {
	if len(assignees) == 0 {
		return
	}
	all := s.All()
	idx := 0
	for _, t := range all {
		if t.Assignee == "" && t.Status != Done {
			s.mu.Lock()
			t.Assignee = assignees[idx%len(assignees)]
			s.mu.Unlock()
			idx++
		}
	}
}

// --- Priority queue (heap-like) ---

type PriorityQueue struct {
	items []*Task
}

func NewPriorityQueue() *PriorityQueue {
	return &PriorityQueue{}
}

func (pq *PriorityQueue) Push(t *Task) {
	pq.items = append(pq.items, t)
	pq.siftUp(len(pq.items) - 1)
}

func (pq *PriorityQueue) Pop() *Task {
	if len(pq.items) == 0 {
		return nil
	}
	top := pq.items[0]
	last := len(pq.items) - 1
	pq.items[0] = pq.items[last]
	pq.items = pq.items[:last]
	if len(pq.items) > 0 {
		pq.siftDown(0)
	}
	return top
}

func (pq *PriorityQueue) Len() int {
	return len(pq.items)
}

func (pq *PriorityQueue) siftUp(i int) {
	for i > 0 {
		parent := (i - 1) / 2
		if pq.items[i].Priority <= pq.items[parent].Priority {
			break
		}
		pq.items[i], pq.items[parent] = pq.items[parent], pq.items[i]
		i = parent
	}
}

func (pq *PriorityQueue) siftDown(i int) {
	n := len(pq.items)
	for {
		left := 2*i + 1
		right := 2*i + 2
		largest := i

		if left < n && pq.items[left].Priority > pq.items[largest].Priority {
			largest = left
		}
		if right < n && pq.items[right].Priority > pq.items[largest].Priority {
			largest = right
		}
		if largest == i {
			break
		}
		pq.items[i], pq.items[largest] = pq.items[largest], pq.items[i]
		i = largest
	}
}

// --- Text report ---

func GenerateReport(store *TaskStore) string {
	stats := store.ComputeStats()
	var b strings.Builder

	b.WriteString("=== Task Report ===\n")
	fmt.Fprintf(&b, "Total: %d\n", stats.Total)
	fmt.Fprintf(&b, "Overdue: %d\n", stats.OverdueCount)
	fmt.Fprintf(&b, "Unassigned: %d\n", stats.Unassigned)
	fmt.Fprintf(&b, "Avg Age: %s\n", stats.AvgAge.Round(time.Second))
	b.WriteString("\nBy Status:\n")

	for _, st := range []Status{Open, InProgress, Blocked, Done} {
		fmt.Fprintf(&b, "  %-12s %d\n", st, stats.ByStatus[st])
	}

	b.WriteString("\nBy Priority:\n")
	for _, p := range []Priority{Critical, High, Medium, Low} {
		fmt.Fprintf(&b, "  %-12s %d\n", p, stats.ByPriority[p])
	}

	b.WriteString("\nTop 5 (by priority):\n")
	pq := NewPriorityQueue()
	for _, t := range store.All() {
		if t.Status != Done {
			pq.Push(t)
		}
	}
	for i := 0; i < 5 && pq.Len() > 0; i++ {
		t := pq.Pop()
		fmt.Fprintf(&b, "  [%d] %s (%s, %s)\n", t.ID, t.Title, t.Priority, t.Status)
	}

	return b.String()
}

// --- Utility algorithms ---

func LevenshteinDistance(a, b string) int {
	la, lb := len(a), len(b)
	if la == 0 {
		return lb
	}
	if lb == 0 {
		return la
	}

	matrix := make([][]int, la+1)
	for i := range matrix {
		matrix[i] = make([]int, lb+1)
		matrix[i][0] = i
	}
	for j := 0; j <= lb; j++ {
		matrix[0][j] = j
	}

	for i := 1; i <= la; i++ {
		for j := 1; j <= lb; j++ {
			cost := 1
			if a[i-1] == b[j-1] {
				cost = 0
			}
			matrix[i][j] = min3(
				matrix[i-1][j]+1,
				matrix[i][j-1]+1,
				matrix[i-1][j-1]+cost,
			)
		}
	}
	return matrix[la][lb]
}

func min3(a, b, c int) int {
	m := a
	if b < m {
		m = b
	}
	if c < m {
		m = c
	}
	return m
}

func FuzzySearch(store *TaskStore, query string, maxDist int) []*Task {
	query = strings.ToLower(query)
	var results []*Task
	for _, t := range store.All() {
		title := strings.ToLower(t.Title)
		if strings.Contains(title, query) || LevenshteinDistance(title, query) <= maxDist {
			results = append(results, t)
		}
	}
	return results
}

// --- Concurrent worker pool ---

type WorkResult struct {
	TaskID  int
	Success bool
	Error   string
}

func ProcessTasksConcurrently(store *TaskStore, ids []int, workers int, process func(*Task) error) []WorkResult {
	jobs := make(chan int, len(ids))
	results := make(chan WorkResult, len(ids))

	var wg sync.WaitGroup
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for id := range jobs {
				t, ok := store.Get(id)
				if !ok {
					results <- WorkResult{TaskID: id, Error: "not found"}
					continue
				}
				if err := process(t); err != nil {
					results <- WorkResult{TaskID: id, Error: err.Error()}
				} else {
					results <- WorkResult{TaskID: id, Success: true}
				}
			}
		}()
	}

	for _, id := range ids {
		jobs <- id
	}
	close(jobs)

	go func() {
		wg.Wait()
		close(results)
	}()

	var out []WorkResult
	for r := range results {
		out = append(out, r)
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i].TaskID < out[j].TaskID
	})
	return out
}

// --- Scoring / ranking ---

func ScoreTask(t *Task) float64 {
	now := time.Now()
	score := float64(t.Priority) * 10.0

	age := now.Sub(t.CreatedAt).Hours() / 24.0
	score += math.Log2(age+1) * 3.0

	if t.DueDate != nil {
		daysUntilDue := t.DueDate.Sub(now).Hours() / 24.0
		if daysUntilDue < 0 {
			score += 50.0
		} else if daysUntilDue < 3 {
			score += 30.0
		} else if daysUntilDue < 7 {
			score += 15.0
		}
	}

	if t.Status == Blocked {
		score *= 0.5
	}

	score += float64(len(t.Subtasks)) * 2.0

	return math.Round(score*100) / 100
}

func RankTasks(store *TaskStore) []*Task {
	tasks := store.All()
	active := make([]*Task, 0)
	for _, t := range tasks {
		if t.Status != Done {
			active = append(active, t)
		}
	}
	sort.Slice(active, func(i, j int) bool {
		return ScoreTask(active[i]) > ScoreTask(active[j])
	})
	return active
}

// --- Stubs for challenge features (to be implemented) ---

func (s *TaskStore) BuildTagIndex() map[string][]*Task {
	panic("CHALLENGE: implement BuildTagIndex")
}

type StatusChange struct {
	FromStatus Status
	ToStatus   Status
	Timestamp  time.Time
}

func (s *TaskStore) TopologicalSort() ([]*Task, error) {
	panic("CHALLENGE: implement TopologicalSort")
}

func (s *TaskStore) UpdateStatus(id int, status Status) {
	panic("CHALLENGE: implement UpdateStatus")
}

func (s *TaskStore) GetHistory(id int) []StatusChange {
	panic("CHALLENGE: implement GetHistory")
}

func (s *TaskStore) MarshalJSON() ([]byte, error) {
	panic("CHALLENGE: implement MarshalJSON")
}

func (s *TaskStore) UnmarshalJSON(data []byte) error {
	panic("CHALLENGE: implement UnmarshalJSON")
}

// --- Main ---

func main() {
	store := NewTaskStore()

	store.Add("Design database schema", Critical)
	store.Add("Implement user auth", High)
	store.Add("Write unit tests", Medium)
	store.Add("Setup CI pipeline", Medium)
	store.Add("Create API docs", Low)
	store.Add("Performance profiling", High)
	store.Add("Security audit", Critical)
	store.Add("Refactor middleware", Medium)
	store.Add("Add logging", Low)
	store.Add("Deploy to staging", High)

	// Assign some
	if t, ok := store.Get(1); ok {
		t.Assignee = "alice"
		t.Tags = []string{"backend", "database"}
	}
	if t, ok := store.Get(2); ok {
		t.Assignee = "bob"
		t.Status = InProgress
		t.Tags = []string{"backend", "security"}
	}
	if t, ok := store.Get(7); ok {
		due := time.Now().Add(-48 * time.Hour)
		t.DueDate = &due
		t.Tags = []string{"security", "compliance"}
	}

	// Subtask relationship
	if t, ok := store.Get(1); ok {
		t.Subtasks = []int{3, 4}
	}

	// Complete some tasks
	store.BulkUpdateStatus([]int{4, 9}, Done)

	// Auto-assign remaining
	store.AutoAssign([]string{"alice", "bob", "charlie"})

	// Report
	fmt.Println(GenerateReport(store))

	// Ranked view
	fmt.Println("=== Ranked Tasks ===")
	for i, t := range RankTasks(store) {
		fmt.Printf("  %d. [%d] %s (score: %.2f, assignee: %s)\n",
			i+1, t.ID, t.Title, ScoreTask(t), t.Assignee)
	}

	// Fuzzy search
	fmt.Println("\n=== Fuzzy Search: 'auth' ===")
	for _, t := range FuzzySearch(store, "auth", 3) {
		fmt.Printf("  [%d] %s\n", t.ID, t.Title)
	}

	// Cycle detection
	fmt.Printf("\nCycle detection (task 1): %v\n", store.HasCycle(1))
}
