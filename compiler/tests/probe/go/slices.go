package foo

// slicing
func firstThree(xs []int) []int {
	return xs[:3]
}

func tail(xs []int) []int {
	return xs[1:]
}

func middle(xs []int, lo, hi int) []int {
	return xs[lo:hi]
}

func last(xs []int) int {
	return xs[len(xs)-1]
}

// append + make
func grow(xs []int, n int) []int {
	out := make([]int, 0, len(xs)+n)
	out = append(out, xs...)
	for i := 0; i < n; i++ {
		out = append(out, 0)
	}
	return out
}

// string slice
func prefix(s string, n int) string {
	return s[:n]
}
