package foo

// goroutine + channel
func Pipeline(in <-chan int, out chan<- int) {
	go func() {
		for x := range in {
			out <- x * 2
		}
		close(out)
	}()
}

// select statement
func Multiplex(a, b <-chan int) int {
	select {
	case x := <-a:
		return x
	case y := <-b:
		return y
	}
}

// defer
func Safe(lock sync.Mutex) {
	lock.Lock()
	defer lock.Unlock()
	doWork()
}
