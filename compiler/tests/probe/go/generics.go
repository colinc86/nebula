package foo

import "golang.org/x/exp/constraints"

// generic function with constraint
func Max[T constraints.Ordered](a, b T) T {
	if a > b {
		return a
	}
	return b
}

// generic slice helpers
func Map[T, U any](xs []T, f func(T) U) []U {
	out := make([]U, len(xs))
	for i, x := range xs {
		out[i] = f(x)
	}
	return out
}

func Filter[T any](xs []T, pred func(T) bool) []T {
	out := make([]T, 0)
	for _, x := range xs {
		if pred(x) {
			out = append(out, x)
		}
	}
	return out
}

// generic container
type Stack[T any] struct {
	items []T
}

func (s *Stack[T]) Push(x T) { s.items = append(s.items, x) }
