package taskpkg

import (
	"reflect"
	"testing"
)

func TestSum(t *testing.T) {
	cases := []struct {
		in   []int
		want int
	}{{[]int{}, 0}, {[]int{1, 2, 3}, 6}, {[]int{-5, 5}, 0}, {[]int{10}, 10}}
	for _, c := range cases {
		if got := Sum(c.in); got != c.want {
			t.Errorf("Sum(%v) = %d, want %d", c.in, got, c.want)
		}
	}
}

func TestMax(t *testing.T) {
	if _, ok := Max([]int{}); ok {
		t.Error("expected ok=false for empty")
	}
	cases := []struct {
		in   []int
		want int
	}{{[]int{1}, 1}, {[]int{3, 1, 2}, 3}, {[]int{-1, -2}, -1}}
	for _, c := range cases {
		if got, ok := Max(c.in); !ok || got != c.want {
			t.Errorf("Max(%v) = %d,%v want %d,true", c.in, got, ok, c.want)
		}
	}
}

func TestFilter(t *testing.T) {
	even := func(n int) bool { return n%2 == 0 }
	got := Filter([]int{1, 2, 3, 4}, even)
	want := []int{2, 4}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("Filter even = %v, want %v", got, want)
	}
	if got := Filter(nil, even); len(got) != 0 {
		t.Errorf("Filter(nil) = %v, want empty", got)
	}
}
