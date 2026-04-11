package main

import (
	"fmt"
	"math"
	"strings"
	"sync"
)

// --- Graph domain ---

type Node struct {
	ID       int
	Label    string
	Weight   float64
	Metadata map[string]string
}

type Edge struct {
	From   int
	To     int
	Cost   float64
	Tags   []string
}

// --- Graph store ---

type Graph struct {
	mu    sync.RWMutex
	nodes map[int]*Node
	edges []Edge
}

func NewGraph() *Graph {
	return &Graph{
		nodes: make(map[int]*Node),
		edges: []Edge{},
	}
}

func (g *Graph) AddNode(id int, label string, weight float64) *Node {
	g.mu.Lock()
	defer g.mu.Unlock()
	n := &Node{
		ID:       id,
		Label:    label,
		Weight:   weight,
		Metadata: make(map[string]string),
	}
	g.nodes[id] = n
	return n
}

func (g *Graph) AddEdge(from, to int, cost float64, tags ...string) error {
	g.mu.Lock()
	defer g.mu.Unlock()
	if _, ok := g.nodes[from]; !ok {
		return fmt.Errorf("node %d does not exist", from)
	}
	if _, ok := g.nodes[to]; !ok {
		return fmt.Errorf("node %d does not exist", to)
	}
	g.edges = append(g.edges, Edge{From: from, To: to, Cost: cost, Tags: tags})
	return nil
}

func (g *Graph) GetNode(id int) (*Node, bool) {
	g.mu.RLock()
	defer g.mu.RUnlock()
	n, ok := g.nodes[id]
	return n, ok
}

func (g *Graph) NodeCount() int {
	g.mu.RLock()
	defer g.mu.RUnlock()
	return len(g.nodes)
}

func (g *Graph) EdgeCount() int {
	g.mu.RLock()
	defer g.mu.RUnlock()
	return len(g.edges)
}

// --- Neighbors ---

func (g *Graph) Neighbors(id int) []int {
	g.mu.RLock()
	defer g.mu.RUnlock()
	var result []int
	for _, e := range g.edges {
		if e.From == id {
			result = append(result, e.To)
		}
	}
	return result
}

func (g *Graph) InDegree(id int) int {
	g.mu.RLock()
	defer g.mu.RUnlock()
	count := 0
	for _, e := range g.edges {
		if e.To == id {
			count++
		}
	}
	return count
}

func (g *Graph) OutDegree(id int) int {
	g.mu.RLock()
	defer g.mu.RUnlock()
	count := 0
	for _, e := range g.edges {
		if e.From == id {
			count++
		}
	}
	return count
}

// --- BFS traversal ---

func (g *Graph) BFS(start int) []int {
	visited := make(map[int]bool)
	queue := []int{start}
	var order []int

	for len(queue) > 0 {
		node := queue[0]
		queue = queue[1:]

		if visited[node] {
			continue
		}
		visited[node] = true
		order = append(order, node)

		for _, n := range g.Neighbors(node) {
			if !visited[n] {
				queue = append(queue, n)
			}
		}
	}
	return order
}

// --- Parse edge list from text ---

func ParseEdgeList(text string) ([]Edge, error) {
	var edges []Edge
	lines := strings.Split(text, "\n")
	for i, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		parts := strings.Fields(line)
		if len(parts) < 3 {
			return nil, fmt.Errorf("line %d: expected 'from to cost', got %q", i+1, line)
		}
		var from, to int
		var cost float64
		if _, err := fmt.Sscanf(parts[0], "%d", &from); err != nil {
			return nil, fmt.Errorf("line %d: invalid from: %v", i+1, err)
		}
		if _, err := fmt.Sscanf(parts[1], "%d", &to); err != nil {
			return nil, fmt.Errorf("line %d: invalid to: %v", i+1, err)
		}
		if _, err := fmt.Sscanf(parts[2], "%f", &cost); err != nil {
			return nil, fmt.Errorf("line %d: invalid cost: %v", i+1, err)
		}
		edges = append(edges, Edge{From: from, To: to, Cost: cost})
	}
	return edges, nil
}

// --- Statistics ---

type Stats struct {
	NodeCount    int
	EdgeCount    int
	AvgDegree    float64
	MaxDegree    int
	MinCost      float64
	MaxCost      float64
	TotalWeight  float64
}

func (g *Graph) ComputeStats() Stats {
	g.mu.RLock()
	defer g.mu.RUnlock()

	stats := Stats{
		NodeCount: len(g.nodes),
		EdgeCount: len(g.edges),
		MinCost:   math.MaxFloat64,
		MaxCost:   -math.MaxFloat64,
	}

	for _, n := range g.nodes {
		stats.TotalWeight += n.Weight
	}

	degrees := make(map[int]int)
	for _, e := range g.edges {
		degrees[e.From]++
		degrees[e.To]++
		if e.Cost < stats.MinCost {
			stats.MinCost = e.Cost
		}
		if e.Cost > stats.MaxCost {
			stats.MaxCost = e.Cost
		}
	}

	totalDeg := 0
	for _, d := range degrees {
		totalDeg += d
		if d > stats.MaxDegree {
			stats.MaxDegree = d
		}
	}
	if len(degrees) > 0 {
		stats.AvgDegree = float64(totalDeg) / float64(len(degrees))
	}
	if stats.EdgeCount == 0 {
		stats.MinCost = 0
		stats.MaxCost = 0
	}
	return stats
}

// --- Stubs for challenge features (to be implemented) ---

// ShortestPath should return the shortest path from `from` to `to` using
// Dijkstra's algorithm, returning the ordered list of node IDs and total cost.
// If no path exists, return nil and an error.
func (g *Graph) ShortestPath(from, to int) ([]int, float64, error) {
	panic("CHALLENGE: implement ShortestPath")
}

// ConnectedComponents should return all connected components in the graph
// (treating edges as undirected). Each component is a list of node IDs.
// The outer list should be sorted by smallest node ID in each component.
func (g *Graph) ConnectedComponents() [][]int {
	panic("CHALLENGE: implement ConnectedComponents")
}

// DetectCycles should return true if the directed graph contains any cycle.
// Use DFS with a recursion stack.
func (g *Graph) DetectCycles() bool {
	panic("CHALLENGE: implement DetectCycles")
}

// FilterByTag should return all edges that have the given tag.
func (g *Graph) FilterByTag(tag string) []Edge {
	panic("CHALLENGE: implement FilterByTag")
}

// --- Helper for error construction ---

func ErrNoPath(from, to int) error {
	return fmt.Errorf("no path from %d to %d", from, to)
}

// --- Main ---

func main() {
	g := NewGraph()

	g.AddNode(1, "Start", 1.0)
	g.AddNode(2, "Middle", 2.0)
	g.AddNode(3, "End", 3.0)
	g.AddNode(4, "Side", 1.5)

	g.AddEdge(1, 2, 5.0, "main")
	g.AddEdge(2, 3, 3.0, "main")
	g.AddEdge(1, 4, 10.0, "alt")
	g.AddEdge(4, 3, 2.0, "alt")

	stats := g.ComputeStats()
	fmt.Printf("Nodes: %d, Edges: %d\n", stats.NodeCount, stats.EdgeCount)
	fmt.Printf("Avg degree: %.2f\n", stats.AvgDegree)

	order := g.BFS(1)
	fmt.Printf("BFS from 1: %v\n", order)
}
