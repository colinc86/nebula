package taskpkg

// Sum returns the sum of all elements in xs.
func Sum(xs []int) int {
	s := 0
	for _, v := range xs {
		s += v
	}
	return s
}

// Max returns the largest element of xs and true. For an empty slice it
// returns 0 and false.
func Max(xs []int) (int, bool) {
	if len(xs) == 0 {
		return 0, false
	}
	m := xs[0]
	for _, v := range xs[1:] {
		if v > m {
			m = v
		}
	}
	return m, true
}

// Filter returns a new slice containing only elements of xs for which keep returns true.
func Filter(xs []int, keep func(int) bool) []int {
	// TODO: implement
	return nil
}
