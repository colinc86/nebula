package foo

import "testing"

// anonymous function passed as arg
func TestSomething(t *testing.T) {
	run(func() { t.Log("hi") })
}

// closure with capture
func makeAdder(n int) func(int) int {
	return func(x int) int { return x + n }
}

// goroutine with anon func
func launchWorker() {
	go func() { process() }()
}
