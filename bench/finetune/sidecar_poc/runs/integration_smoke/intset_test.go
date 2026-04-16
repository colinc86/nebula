package nebulatest

import (
	"sort"
	"testing"
)

func TestAdd(t *testing.T) {
	s := NewIntSet()
	s.Add(1)
	s.Add(2)
	s.Add(1) // duplicate
	if s.Len() != 2 {
		t.Errorf("Len after Add = %d, want 2", s.Len())
	}
	if !s.Contains(1) || !s.Contains(2) {
		t.Errorf("Contains failed")
	}
}

func TestRemove(t *testing.T) {
	s := NewIntSet()
	s.Add(1)
	s.Add(2)
	s.Remove(1)
	if s.Contains(1) {
		t.Errorf("Contains(1) = true after Remove, want false")
	}
	if s.Len() != 1 {
		t.Errorf("Len after Remove = %d, want 1", s.Len())
	}
}

func sortedInts(xs []int) []int {
	out := append([]int(nil), xs...)
	sort.Ints(out)
	return out
}

func eqIntSlice(a, b []int) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// --- New tests you must make pass by adding Union, Intersect, and Difference ---

func TestUnion(t *testing.T) {
	a := NewIntSet()
	a.Add(1)
	a.Add(2)
	a.Add(3)
	b := NewIntSet()
	b.Add(3)
	b.Add(4)
	b.Add(5)
	u := a.Union(b)
	got := sortedInts(u.ToSlice())
	want := []int{1, 2, 3, 4, 5}
	if !eqIntSlice(got, want) {
		t.Errorf("Union = %v, want %v", got, want)
	}
}

func TestIntersect(t *testing.T) {
	a := NewIntSet()
	a.Add(1)
	a.Add(2)
	a.Add(3)
	b := NewIntSet()
	b.Add(2)
	b.Add(3)
	b.Add(4)
	i := a.Intersect(b)
	got := sortedInts(i.ToSlice())
	want := []int{2, 3}
	if !eqIntSlice(got, want) {
		t.Errorf("Intersect = %v, want %v", got, want)
	}
}

func TestDifference(t *testing.T) {
	a := NewIntSet()
	a.Add(1)
	a.Add(2)
	a.Add(3)
	b := NewIntSet()
	b.Add(2)
	b.Add(4)
	d := a.Difference(b)
	got := sortedInts(d.ToSlice())
	want := []int{1, 3}
	if !eqIntSlice(got, want) {
		t.Errorf("Difference = %v, want %v", got, want)
	}
}
