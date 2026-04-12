package nebulatest

import (
	"sort"
	"testing"
)

func TestKeys(t *testing.T) {
	m := map[string]int{"a": 1, "b": 2, "c": 3}
	got := Keys(m)
	sort.Strings(got)
	want := []string{"a", "b", "c"}
	if len(got) != len(want) {
		t.Fatalf("Keys len = %d, want %d", len(got), len(want))
	}
	for i, k := range want {
		if got[i] != k {
			t.Errorf("Keys[%d] = %q, want %q", i, got[i], k)
		}
	}
}

func TestValues(t *testing.T) {
	m := map[string]int{"a": 1, "b": 2, "c": 3}
	got := Values(m)
	sort.Ints(got)
	want := []int{1, 2, 3}
	if len(got) != len(want) {
		t.Fatalf("Values len = %d, want %d", len(got), len(want))
	}
	for i, v := range want {
		if got[i] != v {
			t.Errorf("Values[%d] = %d, want %d", i, got[i], v)
		}
	}
}

func TestMerge(t *testing.T) {
	a := map[string]int{"x": 1, "y": 2}
	b := map[string]int{"y": 20, "z": 30}
	got := Merge(a, b)
	want := map[string]int{"x": 1, "y": 20, "z": 30}
	if len(got) != len(want) {
		t.Fatalf("Merge len = %d, want %d", len(got), len(want))
	}
	for k, v := range want {
		if got[k] != v {
			t.Errorf("Merge[%q] = %d, want %d", k, got[k], v)
		}
	}
}

func TestInvert(t *testing.T) {
	m := map[string]int{"a": 1, "b": 2, "c": 3}
	got := Invert(m)
	want := map[int]string{1: "a", 2: "b", 3: "c"}
	if len(got) != len(want) {
		t.Fatalf("Invert len = %d, want %d", len(got), len(want))
	}
	for k, v := range want {
		if got[k] != v {
			t.Errorf("Invert[%d] = %q, want %q", k, got[k], v)
		}
	}
}
