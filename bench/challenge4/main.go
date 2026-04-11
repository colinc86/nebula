package main

import (
	"fmt"
	"sync"
	"time"
)

// --- State machine domain ---

type State string

const (
	StateIdle     State = "idle"
	StateRunning  State = "running"
	StatePaused   State = "paused"
	StateStopped  State = "stopped"
	StateError    State = "error"
)

type Event string

const (
	EventStart  Event = "start"
	EventPause  Event = "pause"
	EventResume Event = "resume"
	EventStop   Event = "stop"
	EventFail   Event = "fail"
	EventReset  Event = "reset"
)

type Transition struct {
	From  State
	Event Event
	To    State
}

type TransitionLog struct {
	From      State
	To        State
	Event     Event
	Timestamp time.Time
}

// --- Machine ---

type Machine struct {
	mu          sync.RWMutex
	current     State
	transitions []Transition
	history     []TransitionLog
	callbacks   map[State][]func(State)
}

func NewMachine(initial State) *Machine {
	return &Machine{
		current:     initial,
		transitions: []Transition{},
		history:     []TransitionLog{},
		callbacks:   make(map[State][]func(State)),
	}
}

func (m *Machine) AddTransition(from State, event Event, to State) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.transitions = append(m.transitions, Transition{From: from, Event: event, To: to})
}

func (m *Machine) Current() State {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.current
}

func (m *Machine) OnEnter(state State, callback func(State)) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.callbacks[state] = append(m.callbacks[state], callback)
}

func (m *Machine) History() []TransitionLog {
	m.mu.RLock()
	defer m.mu.RUnlock()
	result := make([]TransitionLog, len(m.history))
	copy(result, m.history)
	return result
}

// Fire the given event and return the new state, or an error if the
// transition is not allowed from the current state.
func (m *Machine) Fire(event Event) (State, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	for _, t := range m.transitions {
		if t.From == m.current && t.Event == event {
			old := m.current
			m.current = t.To
			m.history = append(m.history, TransitionLog{
				From:      old,
				To:        t.To,
				Event:     event,
				Timestamp: time.Now(),
			})
			// Call callbacks outside the loop-wait
			callbacks := m.callbacks[t.To]
			for _, cb := range callbacks {
				cb(old)
			}
			return t.To, nil
		}
	}
	return m.current, fmt.Errorf("no transition from %s on event %s", m.current, event)
}

// --- Helpers ---

func (m *Machine) CanFire(event Event) bool {
	m.mu.RLock()
	defer m.mu.RUnlock()
	for _, t := range m.transitions {
		if t.From == m.current && t.Event == event {
			return true
		}
	}
	return false
}

func (m *Machine) AllowedEvents() []Event {
	m.mu.RLock()
	defer m.mu.RUnlock()
	var events []Event
	seen := make(map[Event]bool)
	for _, t := range m.transitions {
		if t.From == m.current && !seen[t.Event] {
			events = append(events, t.Event)
			seen[t.Event] = true
		}
	}
	return events
}

// --- Stubs for challenge features ---

// ReachableStates returns the set of all states reachable from the given
// start state via any sequence of transitions. The result should include
// the start state itself.
func (m *Machine) ReachableStates(start State) []State {
	panic("CHALLENGE: implement ReachableStates")
}

// IsDeadlock returns true if the current state has no outgoing transitions
// (i.e., no events can be fired from it).
func (m *Machine) IsDeadlock() bool {
	panic("CHALLENGE: implement IsDeadlock")
}

// ShortestPath returns the shortest sequence of events that takes the
// machine from `from` to `to` (BFS). Returns nil if unreachable.
func (m *Machine) ShortestPath(from, to State) []Event {
	panic("CHALLENGE: implement ShortestPath")
}

// ValidateDeterministic returns true if the machine is deterministic —
// i.e., for every (state, event) pair there is at most one transition.
func (m *Machine) ValidateDeterministic() bool {
	panic("CHALLENGE: implement ValidateDeterministic")
}

// --- Main ---

func main() {
	m := NewMachine(StateIdle)
	m.AddTransition(StateIdle, EventStart, StateRunning)
	m.AddTransition(StateRunning, EventPause, StatePaused)
	m.AddTransition(StatePaused, EventResume, StateRunning)
	m.AddTransition(StateRunning, EventStop, StateStopped)
	m.AddTransition(StateRunning, EventFail, StateError)
	m.AddTransition(StateError, EventReset, StateIdle)

	fmt.Printf("Initial state: %s\n", m.Current())
	m.Fire(EventStart)
	fmt.Printf("After start: %s\n", m.Current())
	m.Fire(EventPause)
	fmt.Printf("After pause: %s\n", m.Current())
	fmt.Printf("Allowed events: %v\n", m.AllowedEvents())
}
