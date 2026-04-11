package main

import (
	"testing"
)

func seedMachine() *Machine {
	m := NewMachine(StateIdle)
	m.AddTransition(StateIdle, EventStart, StateRunning)
	m.AddTransition(StateRunning, EventPause, StatePaused)
	m.AddTransition(StatePaused, EventResume, StateRunning)
	m.AddTransition(StateRunning, EventStop, StateStopped)
	m.AddTransition(StateRunning, EventFail, StateError)
	m.AddTransition(StateError, EventReset, StateIdle)
	m.AddTransition(StatePaused, EventStop, StateStopped)
	return m
}

// --- Existing tests ---

func TestInitial(t *testing.T) {
	m := NewMachine(StateIdle)
	if m.Current() != StateIdle {
		t.Fatal("initial state wrong")
	}
}

func TestFire(t *testing.T) {
	m := seedMachine()
	if _, err := m.Fire(EventStart); err != nil {
		t.Fatal(err)
	}
	if m.Current() != StateRunning {
		t.Fatal("state not updated")
	}
}

func TestFireInvalid(t *testing.T) {
	m := seedMachine()
	if _, err := m.Fire(EventPause); err == nil {
		t.Fatal("expected error for invalid transition")
	}
	if m.Current() != StateIdle {
		t.Fatal("state changed on invalid transition")
	}
}

func TestCanFire(t *testing.T) {
	m := seedMachine()
	if !m.CanFire(EventStart) {
		t.Fatal("should allow start from idle")
	}
	if m.CanFire(EventPause) {
		t.Fatal("should not allow pause from idle")
	}
}

func TestAllowedEvents(t *testing.T) {
	m := seedMachine()
	events := m.AllowedEvents()
	if len(events) != 1 || events[0] != EventStart {
		t.Fatalf("wrong allowed events from idle: %v", events)
	}
}

func TestHistory(t *testing.T) {
	m := seedMachine()
	m.Fire(EventStart)
	m.Fire(EventPause)
	m.Fire(EventResume)
	hist := m.History()
	if len(hist) != 3 {
		t.Fatalf("expected 3 history entries, got %d", len(hist))
	}
	if hist[0].Event != EventStart {
		t.Fatal("history order wrong")
	}
}

func TestOnEnter(t *testing.T) {
	m := seedMachine()
	entered := false
	m.OnEnter(StateRunning, func(from State) {
		entered = true
	})
	m.Fire(EventStart)
	if !entered {
		t.Fatal("OnEnter callback not fired")
	}
}

// --- CHALLENGE TESTS ---

func TestChallenge_ReachableStates(t *testing.T) {
	t.Skip("CHALLENGE: Implement ReachableStates")

	m := seedMachine()
	reachable := m.ReachableStates(StateIdle)

	// From idle, we can reach: idle, running, paused, stopped, error
	if len(reachable) != 5 {
		t.Fatalf("expected 5 reachable states, got %d: %v", len(reachable), reachable)
	}

	// Verify idle is in the set
	found := false
	for _, s := range reachable {
		if s == StateIdle {
			found = true
			break
		}
	}
	if !found {
		t.Fatal("start state not in reachable set")
	}
}

func TestChallenge_IsDeadlock(t *testing.T) {
	t.Skip("CHALLENGE: Implement IsDeadlock")

	m := seedMachine()
	if m.IsDeadlock() {
		t.Fatal("idle should not be a deadlock")
	}

	// Move to stopped which has no outgoing transitions in our seed
	m.Fire(EventStart)
	m.Fire(EventStop)
	if !m.IsDeadlock() {
		t.Fatal("stopped state has no transitions, should be deadlock")
	}
}

func TestChallenge_ShortestPath(t *testing.T) {
	t.Skip("CHALLENGE: Implement ShortestPath")

	m := seedMachine()

	// Shortest path idle → stopped is start, stop (2 events)
	path := m.ShortestPath(StateIdle, StateStopped)
	if len(path) != 2 {
		t.Fatalf("expected path of 2 events, got %d: %v", len(path), path)
	}
	if path[0] != EventStart || path[1] != EventStop {
		t.Fatalf("wrong path: %v", path)
	}

	// Unreachable
	m2 := NewMachine(StateIdle)
	m2.AddTransition(StateIdle, EventStart, StateRunning)
	// no path from idle to stopped
	unreachable := m2.ShortestPath(StateIdle, StateStopped)
	if unreachable != nil {
		t.Fatalf("expected nil for unreachable, got %v", unreachable)
	}
}

func TestChallenge_ValidateDeterministic(t *testing.T) {
	t.Skip("CHALLENGE: Implement ValidateDeterministic")

	m := seedMachine()
	if !m.ValidateDeterministic() {
		t.Fatal("seeded machine should be deterministic")
	}

	// Create a non-deterministic machine
	m2 := NewMachine(StateIdle)
	m2.AddTransition(StateIdle, EventStart, StateRunning)
	m2.AddTransition(StateIdle, EventStart, StatePaused) // duplicate!
	if m2.ValidateDeterministic() {
		t.Fatal("should detect non-determinism")
	}
}
