package foo

func upto(n int) int {
	total := 0
	for i := 0; i < n; i++ {
		total += i
	}
	return total
}

func rangeSlice(xs []int) int {
	total := 0
	for i, x := range xs {
		total += i + x
	}
	return total
}

func rangeMap(m map[string]int) int {
	total := 0
	for _, v := range m {
		total += v
	}
	return total
}

func rangeString(s string) int {
	total := 0
	for _, r := range s {
		total += int(r)
	}
	return total
}

// infinite with break
func countUntil(stop int) int {
	i := 0
	for {
		if i >= stop {
			break
		}
		i++
	}
	return i
}
