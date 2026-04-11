package main

import (
	"testing"
)

func seedGraph() *Graph {
	g := NewGraph()
	g.AddNode(1, "A", 1.0)
	g.AddNode(2, "B", 2.0)
	g.AddNode(3, "C", 3.0)
	g.AddNode(4, "D", 4.0)
	g.AddNode(5, "E", 5.0)
	return g
}

// --- Existing behavior tests (must still pass) ---

func TestAddNode(t *testing.T) {
	g := NewGraph()
	n := g.AddNode(1, "test", 1.5)
	if n.ID != 1 || n.Label != "test" || n.Weight != 1.5 {
		t.Fatal("AddNode did not set fields correctly")
	}
	if g.NodeCount() != 1 {
		t.Fatal("NodeCount wrong")
	}
}

func TestAddEdge(t *testing.T) {
	g := seedGraph()
	if err := g.AddEdge(1, 2, 5.0, "primary"); err != nil {
		t.Fatal(err)
	}
	if g.EdgeCount() != 1 {
		t.Fatal("EdgeCount wrong")
	}
	if err := g.AddEdge(1, 99, 1.0); err == nil {
		t.Fatal("expected error for non-existent node")
	}
}

func TestNeighbors(t *testing.T) {
	g := seedGraph()
	g.AddEdge(1, 2, 1.0)
	g.AddEdge(1, 3, 2.0)
	g.AddEdge(2, 3, 3.0)

	n := g.Neighbors(1)
	if len(n) != 2 {
		t.Fatalf("expected 2 neighbors of 1, got %d", len(n))
	}
}

func TestInOutDegree(t *testing.T) {
	g := seedGraph()
	g.AddEdge(1, 2, 1.0)
	g.AddEdge(1, 3, 2.0)
	g.AddEdge(2, 3, 3.0)

	if g.OutDegree(1) != 2 {
		t.Fatal("OutDegree(1) wrong")
	}
	if g.InDegree(3) != 2 {
		t.Fatal("InDegree(3) wrong")
	}
}

func TestBFS(t *testing.T) {
	g := seedGraph()
	g.AddEdge(1, 2, 1.0)
	g.AddEdge(1, 3, 1.0)
	g.AddEdge(2, 4, 1.0)
	g.AddEdge(3, 5, 1.0)

	order := g.BFS(1)
	if len(order) != 5 {
		t.Fatalf("expected 5 nodes in BFS, got %d", len(order))
	}
	if order[0] != 1 {
		t.Fatal("BFS should start at 1")
	}
}

func TestParseEdgeList(t *testing.T) {
	input := `# comment line
1 2 5.0
2 3 3.5
3 4 1.0
`
	edges, err := ParseEdgeList(input)
	if err != nil {
		t.Fatal(err)
	}
	if len(edges) != 3 {
		t.Fatalf("expected 3 edges, got %d", len(edges))
	}
	if edges[0].From != 1 || edges[0].To != 2 || edges[0].Cost != 5.0 {
		t.Fatal("first edge parsed wrong")
	}
}

func TestComputeStats(t *testing.T) {
	g := seedGraph()
	g.AddEdge(1, 2, 5.0)
	g.AddEdge(2, 3, 10.0)
	g.AddEdge(3, 4, 2.5)

	stats := g.ComputeStats()
	if stats.NodeCount != 5 || stats.EdgeCount != 3 {
		t.Fatalf("counts wrong: %+v", stats)
	}
	if stats.MinCost != 2.5 || stats.MaxCost != 10.0 {
		t.Fatalf("cost range wrong: %+v", stats)
	}
}

// --- CHALLENGE TESTS ---

func TestChallenge_ShortestPath(t *testing.T) {
	t.Skip("CHALLENGE: Implement ShortestPath — remove this Skip to activate")

	g := seedGraph()
	g.AddEdge(1, 2, 5.0)
	g.AddEdge(2, 3, 3.0)
	g.AddEdge(1, 3, 10.0)
	g.AddEdge(3, 4, 1.0)
	g.AddEdge(1, 4, 100.0)

	path, cost, err := g.ShortestPath(1, 4)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	// Shortest path: 1 -> 2 -> 3 -> 4 = 5 + 3 + 1 = 9
	if cost != 9.0 {
		t.Fatalf("expected cost 9.0, got %v", cost)
	}
	if len(path) != 4 || path[0] != 1 || path[3] != 4 {
		t.Fatalf("expected path [1,2,3,4], got %v", path)
	}

	// Test unreachable
	g.AddNode(99, "Isolated", 1.0)
	_, _, err = g.ShortestPath(1, 99)
	if err == nil {
		t.Fatal("expected error for unreachable node")
	}
}

func TestChallenge_ConnectedComponents(t *testing.T) {
	t.Skip("CHALLENGE: Implement ConnectedComponents — remove this Skip to activate")

	g := seedGraph()
	// Component 1: {1, 2, 3}
	g.AddEdge(1, 2, 1.0)
	g.AddEdge(2, 3, 1.0)
	// Component 2: {4, 5}
	g.AddEdge(4, 5, 1.0)

	components := g.ConnectedComponents()
	if len(components) != 2 {
		t.Fatalf("expected 2 components, got %d", len(components))
	}
	if len(components[0]) != 3 {
		t.Fatalf("first component size wrong: %v", components[0])
	}
	if len(components[1]) != 2 {
		t.Fatalf("second component size wrong: %v", components[1])
	}
}

func TestChallenge_DetectCycles(t *testing.T) {
	t.Skip("CHALLENGE: Implement DetectCycles — remove this Skip to activate")

	g := seedGraph()
	g.AddEdge(1, 2, 1.0)
	g.AddEdge(2, 3, 1.0)
	g.AddEdge(3, 4, 1.0)

	if g.DetectCycles() {
		t.Fatal("false positive cycle")
	}

	// Create cycle: 4 -> 1
	g.AddEdge(4, 1, 1.0)
	if !g.DetectCycles() {
		t.Fatal("missed cycle")
	}
}

func TestChallenge_FilterByTag(t *testing.T) {
	t.Skip("CHALLENGE: Implement FilterByTag — remove this Skip to activate")

	g := seedGraph()
	g.AddEdge(1, 2, 1.0, "red", "primary")
	g.AddEdge(2, 3, 1.0, "blue")
	g.AddEdge(3, 4, 1.0, "red")

	red := g.FilterByTag("red")
	if len(red) != 2 {
		t.Fatalf("expected 2 red edges, got %d", len(red))
	}

	blue := g.FilterByTag("blue")
	if len(blue) != 1 {
		t.Fatalf("expected 1 blue edge, got %d", len(blue))
	}

	none := g.FilterByTag("nonexistent")
	if len(none) != 0 {
		t.Fatalf("expected 0 edges, got %d", len(none))
	}
}
