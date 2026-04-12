package nebulatest

import (
	"reflect"
	"testing"
)

func TestUnique(t *testing.T) {
	cases := []struct {
		in   []int
		want []int
	}{
		{[]int{}, []int{}},
		{[]int{1}, []int{1}},
		{[]int{1, 2, 3}, []int{1, 2, 3}},
		{[]int{1, 1, 2, 2, 3, 3}, []int{1, 2, 3}},
		{[]int{3, 1, 2, 1, 3}, []int{3, 1, 2}},
	}
	for _, c := range cases {
		got := Unique(c.in)
		if len(got) == 0 && len(c.want) == 0 {
			continue
		}
		if !reflect.DeepEqual(got, c.want) {
			t.Errorf("Unique(%v) = %v, want %v", c.in, got, c.want)
		}
	}
}

func TestChunk(t *testing.T) {
	cases := []struct {
		in   []int
		size int
		want [][]int
	}{
		{[]int{1, 2, 3, 4}, 2, [][]int{{1, 2}, {3, 4}}},
		{[]int{1, 2, 3, 4, 5}, 2, [][]int{{1, 2}, {3, 4}, {5}}},
		{[]int{1, 2, 3}, 5, [][]int{{1, 2, 3}}},
		{[]int{}, 2, nil},
		{[]int{1, 2, 3}, 0, nil},
	}
	for _, c := range cases {
		got := Chunk(c.in, c.size)
		if len(got) == 0 && len(c.want) == 0 {
			continue
		}
		if !reflect.DeepEqual(got, c.want) {
			t.Errorf("Chunk(%v, %d) = %v, want %v", c.in, c.size, got, c.want)
		}
	}
}

func TestContains(t *testing.T) {
	cases := []struct {
		in     []int
		target int
		want   bool
	}{
		{[]int{}, 1, false},
		{[]int{1, 2, 3}, 2, true},
		{[]int{1, 2, 3}, 4, false},
		{[]int{5}, 5, true},
	}
	for _, c := range cases {
		if got := Contains(c.in, c.target); got != c.want {
			t.Errorf("Contains(%v, %d) = %v, want %v", c.in, c.target, got, c.want)
		}
	}
}

func TestReverseInts(t *testing.T) {
	cases := []struct {
		in, want []int
	}{
		{[]int{}, []int{}},
		{[]int{1}, []int{1}},
		{[]int{1, 2, 3}, []int{3, 2, 1}},
		{[]int{1, 2, 3, 4}, []int{4, 3, 2, 1}},
	}
	for _, c := range cases {
		got := ReverseInts(c.in)
		if len(got) == 0 && len(c.want) == 0 {
			continue
		}
		if !reflect.DeepEqual(got, c.want) {
			t.Errorf("ReverseInts(%v) = %v, want %v", c.in, got, c.want)
		}
	}
}
