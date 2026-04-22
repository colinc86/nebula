package foo

// slice of strings
func joinStrs(xs []string, sep string) string {
	out := ""
	for i, s := range xs {
		if i > 0 {
			out += sep
		}
		out += s
	}
	return out
}

// map[K]V
func countWords(words []string) map[string]int {
	out := make(map[string]int)
	for _, w := range words {
		out[w]++
	}
	return out
}

// nested collections
func grid(n int) [][]int {
	m := make([][]int, n)
	for i := range m {
		m[i] = make([]int, n)
	}
	return m
}

// array (fixed-size)
func firstThreeArr(xs [3]int) [3]int {
	return xs
}
