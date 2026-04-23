package foo

import "testing"

func runFn(f func()) { f() }
func doWork()        { _ = 0 }

// anonymous function passed as arg
func TestSomething(t *testing.T) {
	runFn(func() { t.Log("hi") })
}

// closure with capture
func makeAdder(n int) func(int) int {
	return func(x int) int { return x + n }
}

// goroutine with anon func
func launchWorker() {
	go func() { doWork() }()
}
