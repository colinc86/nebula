package gdsp

func Reverse(xs []float64) []float64 {
	n := len(xs)
	out := make([]float64, n)
	for i, v := range xs {
		out[n-1-i] = v
	}
	return out
}
