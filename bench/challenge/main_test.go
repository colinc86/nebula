package main

import (
	"strings"
	"testing"
	"time"
)

func seedStore() *TaskStore {
	s := NewTaskStore()
	s.Add("Design database schema", Critical)
	s.Add("Implement user auth", High)
	s.Add("Write unit tests", Medium)
	s.Add("Setup CI pipeline", Medium)
	s.Add("Create API docs", Low)
	return s
}

// --- Existing behavior tests (must still pass after modifications) ---

func TestAddAndGet(t *testing.T) {
	s := NewTaskStore()
	task := s.Add("Test task", High)
	if task.ID != 1 {
		t.Fatalf("expected ID 1, got %d", task.ID)
	}
	got, ok := s.Get(1)
	if !ok || got.Title != "Test task" {
		t.Fatal("Get failed")
	}
}

func TestRemove(t *testing.T) {
	s := seedStore()
	if !s.Remove(3) {
		t.Fatal("Remove returned false for existing task")
	}
	if s.Remove(99) {
		t.Fatal("Remove returned true for non-existent task")
	}
	if _, ok := s.Get(3); ok {
		t.Fatal("Task still exists after remove")
	}
}

func TestAllSorted(t *testing.T) {
	s := seedStore()
	all := s.All()
	for i := 1; i < len(all); i++ {
		if all[i].ID < all[i-1].ID {
			t.Fatal("All() not sorted by ID")
		}
	}
}

func TestSearch(t *testing.T) {
	s := seedStore()
	if t2, ok := s.Get(1); ok {
		t2.Tags = []string{"backend"}
		t2.Assignee = "alice"
	}

	p := Critical
	results := s.Search(Filter{Priority: &p})
	if len(results) != 1 || results[0].ID != 1 {
		t.Fatalf("Priority filter: expected 1 result, got %d", len(results))
	}

	results = s.Search(Filter{Tag: "backend"})
	if len(results) != 1 {
		t.Fatalf("Tag filter: expected 1 result, got %d", len(results))
	}

	results = s.Search(Filter{Assignee: "alice"})
	if len(results) != 1 {
		t.Fatalf("Assignee filter: expected 1 result, got %d", len(results))
	}
}

func TestOverdueFilter(t *testing.T) {
	s := NewTaskStore()
	task := s.Add("Overdue task", High)
	past := time.Now().Add(-24 * time.Hour)
	task.DueDate = &past

	results := s.Search(Filter{Overdue: true})
	if len(results) != 1 {
		t.Fatalf("expected 1 overdue, got %d", len(results))
	}

	// Done tasks should not appear as overdue
	task.Status = Done
	results = s.Search(Filter{Overdue: true})
	if len(results) != 0 {
		t.Fatalf("done task should not be overdue, got %d", len(results))
	}
}

func TestComputeStats(t *testing.T) {
	s := seedStore()
	stats := s.ComputeStats()
	if stats.Total != 5 {
		t.Fatalf("expected 5 total, got %d", stats.Total)
	}
	if stats.ByStatus[Open] != 5 {
		t.Fatalf("expected 5 open, got %d", stats.ByStatus[Open])
	}
	if stats.Unassigned != 5 {
		t.Fatalf("expected 5 unassigned, got %d", stats.Unassigned)
	}
}

func TestBulkUpdateStatus(t *testing.T) {
	s := seedStore()
	updated, failed := s.BulkUpdateStatus([]int{1, 2, 99}, Done)
	if updated != 2 {
		t.Fatalf("expected 2 updated, got %d", updated)
	}
	if len(failed) != 1 || failed[0] != 99 {
		t.Fatalf("expected [99] failed, got %v", failed)
	}
}

func TestAutoAssign(t *testing.T) {
	s := seedStore()
	s.AutoAssign([]string{"alice", "bob"})
	for _, task := range s.All() {
		if task.Assignee == "" {
			t.Fatalf("task %d still unassigned", task.ID)
		}
	}
}

func TestPriorityQueue(t *testing.T) {
	pq := NewPriorityQueue()
	s := seedStore()
	for _, task := range s.All() {
		pq.Push(task)
	}

	prev := Critical + 1
	for pq.Len() > 0 {
		task := pq.Pop()
		if task.Priority > prev {
			t.Fatal("priority queue order violated")
		}
		prev = task.Priority
	}
}

func TestPriorityQueueEmpty(t *testing.T) {
	pq := NewPriorityQueue()
	if pq.Pop() != nil {
		t.Fatal("Pop on empty queue should return nil")
	}
}

func TestLevenshtein(t *testing.T) {
	cases := []struct{ a, b string; want int }{
		{"", "", 0},
		{"abc", "", 3},
		{"", "abc", 3},
		{"kitten", "sitting", 3},
		{"same", "same", 0},
	}
	for _, c := range cases {
		got := LevenshteinDistance(c.a, c.b)
		if got != c.want {
			t.Errorf("Levenshtein(%q, %q) = %d, want %d", c.a, c.b, got, c.want)
		}
	}
}

func TestFuzzySearch(t *testing.T) {
	s := seedStore()
	results := FuzzySearch(s, "auth", 3)
	if len(results) != 1 || results[0].ID != 2 {
		t.Fatalf("expected to find 'Implement user auth', got %d results", len(results))
	}
}

func TestCycleDetection(t *testing.T) {
	s := NewTaskStore()
	s.Add("A", Low)
	s.Add("B", Low)
	s.Add("C", Low)

	// A -> B -> C (no cycle)
	if a, ok := s.Get(1); ok {
		a.Subtasks = []int{2}
	}
	if b, ok := s.Get(2); ok {
		b.Subtasks = []int{3}
	}
	if s.HasCycle(1) {
		t.Fatal("false positive cycle")
	}

	// Create cycle: C -> A
	if c, ok := s.Get(3); ok {
		c.Subtasks = []int{1}
	}
	if !s.HasCycle(1) {
		t.Fatal("missed cycle")
	}
}

func TestConcurrentProcessing(t *testing.T) {
	s := seedStore()
	ids := []int{1, 2, 3, 4, 5}
	results := ProcessTasksConcurrently(s, ids, 3, func(task *Task) error {
		return nil
	})
	if len(results) != 5 {
		t.Fatalf("expected 5 results, got %d", len(results))
	}
	for _, r := range results {
		if !r.Success {
			t.Fatalf("task %d failed: %s", r.TaskID, r.Error)
		}
	}
}

func TestScoreTask(t *testing.T) {
	task := &Task{
		Priority:  Critical,
		Status:    Open,
		CreatedAt: time.Now().Add(-48 * time.Hour),
	}
	score := ScoreTask(task)
	if score <= 0 {
		t.Fatalf("expected positive score, got %.2f", score)
	}

	// Blocked tasks should score lower
	task.Status = Blocked
	blockedScore := ScoreTask(task)
	if blockedScore >= score {
		t.Fatalf("blocked score %.2f should be less than open score %.2f", blockedScore, score)
	}
}

func TestGenerateReport(t *testing.T) {
	s := seedStore()
	report := GenerateReport(s)
	if !strings.Contains(report, "Task Report") {
		t.Fatal("report missing header")
	}
	if !strings.Contains(report, "Total: 5") {
		t.Fatal("report missing total")
	}
}

// --- CHALLENGE TESTS ---
// These tests define the features Claude must implement.
// They start FAILING. The challenge is to make them pass.

func TestChallenge_TagIndex(t *testing.T) {
	t.Skip("CHALLENGE: Implement TagIndex — remove this Skip to activate")

	s := NewTaskStore()
	a := s.Add("Backend work", High)
	a.Tags = []string{"backend", "api"}
	b := s.Add("Frontend work", Medium)
	b.Tags = []string{"frontend", "api"}
	c := s.Add("Database migration", Critical)
	c.Tags = []string{"backend", "database"}

	// BuildTagIndex should return map[string][]*Task
	idx := s.BuildTagIndex()

	if len(idx["backend"]) != 2 {
		t.Fatalf("expected 2 backend tasks, got %d", len(idx["backend"]))
	}
	if len(idx["api"]) != 2 {
		t.Fatalf("expected 2 api tasks, got %d", len(idx["api"]))
	}
	if len(idx["database"]) != 1 {
		t.Fatalf("expected 1 database task, got %d", len(idx["database"]))
	}
	if len(idx["frontend"]) != 1 {
		t.Fatalf("expected 1 frontend task, got %d", len(idx["frontend"]))
	}
}

func TestChallenge_DependencyOrder(t *testing.T) {
	t.Skip("CHALLENGE: Implement TopologicalSort — remove this Skip to activate")

	s := NewTaskStore()
	s.Add("Foundation", Low)    // 1
	s.Add("Walls", Low)        // 2
	s.Add("Roof", Low)         // 3
	s.Add("Paint", Low)        // 4

	// Roof depends on Walls, Walls depends on Foundation, Paint depends on Roof
	if t2, ok := s.Get(3); ok {
		t2.Subtasks = []int{2}
	}
	if t2, ok := s.Get(2); ok {
		t2.Subtasks = []int{1}
	}
	if t2, ok := s.Get(4); ok {
		t2.Subtasks = []int{3}
	}

	order, err := s.TopologicalSort()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Foundation must come before Walls, Walls before Roof, Roof before Paint
	pos := make(map[int]int)
	for i, task := range order {
		pos[task.ID] = i
	}
	if pos[1] >= pos[2] {
		t.Fatal("Foundation must come before Walls")
	}
	if pos[2] >= pos[3] {
		t.Fatal("Walls must come before Roof")
	}
	if pos[3] >= pos[4] {
		t.Fatal("Roof must come before Paint")
	}

	// Test cycle detection
	if c, ok := s.Get(1); ok {
		c.Subtasks = []int{4} // creates cycle
	}
	_, err = s.TopologicalSort()
	if err == nil {
		t.Fatal("expected error for cyclic graph")
	}
}

func TestChallenge_TaskHistory(t *testing.T) {
	t.Skip("CHALLENGE: Implement TaskHistory — remove this Skip to activate")

	s := NewTaskStore()
	task := s.Add("Track me", Medium)

	// Changing status should record history
	s.UpdateStatus(task.ID, InProgress)
	s.UpdateStatus(task.ID, Blocked)
	s.UpdateStatus(task.ID, InProgress)
	s.UpdateStatus(task.ID, Done)

	history := s.GetHistory(task.ID)
	if len(history) != 4 {
		t.Fatalf("expected 4 history entries, got %d", len(history))
	}

	// Each entry should have: FromStatus, ToStatus, Timestamp
	if history[0].FromStatus != Open || history[0].ToStatus != InProgress {
		t.Fatal("first transition wrong")
	}
	if history[3].ToStatus != Done {
		t.Fatal("last transition should be to Done")
	}

	// Timestamps should be in order
	for i := 1; i < len(history); i++ {
		if history[i].Timestamp.Before(history[i-1].Timestamp) {
			t.Fatal("timestamps out of order")
		}
	}
}

func TestChallenge_Serialization(t *testing.T) {
	t.Skip("CHALLENGE: Implement JSON serialization — remove this Skip to activate")

	s := seedStore()
	if t2, ok := s.Get(1); ok {
		t2.Assignee = "alice"
		t2.Tags = []string{"backend"}
		due := time.Date(2026, 5, 1, 0, 0, 0, 0, time.UTC)
		t2.DueDate = &due
	}

	// Serialize to JSON and back
	data, err := s.MarshalJSON()
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}

	s2 := NewTaskStore()
	err = s2.UnmarshalJSON(data)
	if err != nil {
		t.Fatalf("unmarshal error: %v", err)
	}

	// Verify round-trip
	if len(s2.All()) != len(s.All()) {
		t.Fatal("task count mismatch after round-trip")
	}

	orig, _ := s.Get(1)
	copy_, _ := s2.Get(1)
	if copy_.Title != orig.Title {
		t.Fatal("title mismatch")
	}
	if copy_.Assignee != orig.Assignee {
		t.Fatal("assignee mismatch")
	}
	if copy_.Priority != orig.Priority {
		t.Fatal("priority mismatch")
	}
	if len(copy_.Tags) != len(orig.Tags) {
		t.Fatal("tags mismatch")
	}
	if copy_.DueDate == nil || !copy_.DueDate.Equal(*orig.DueDate) {
		t.Fatal("due date mismatch")
	}

	// NextID should be preserved
	newTask := s2.Add("After roundtrip", Low)
	if newTask.ID <= 5 {
		t.Fatal("NextID not preserved — IDs would collide")
	}
}
