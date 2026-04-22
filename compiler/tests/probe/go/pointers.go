package foo

// pointer receiver
type Counter struct {
	n int
}

func (c *Counter) Inc() {
	c.n++
}

// pointer arg
func zero(p *int) {
	*p = 0
}

// nil pointer
func maybeDeref(p *int) int {
	if p == nil {
		return -1
	}
	return *p
}

// & and * operators
func pairPtrs() (*int, *int) {
	a, b := 1, 2
	return &a, &b
}
