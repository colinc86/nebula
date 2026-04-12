package taskpkg

import "testing"

func TestMergeStrings(t *testing.T) {
	a := map[string]string{"x": "1", "y": "2"}
	b := map[string]string{"y": "20", "z": "30"}
	got := MergeStrings(a, b)
	want := map[string]string{"x": "1", "y": "20", "z": "30"}
	if len(got) != len(want) {
		t.Fatalf("len = %d, want %d", len(got), len(want))
	}
	for k, v := range want {
		if got[k] != v {
			t.Errorf("[%s] = %q, want %q", k, got[k], v)
		}
	}
}

func TestCountWords(t *testing.T) {
	got := CountWords("the quick brown fox jumps over the lazy dog the")
	if got["the"] != 3 {
		t.Errorf("the = %d, want 3", got["the"])
	}
	if got["fox"] != 1 {
		t.Errorf("fox = %d, want 1", got["fox"])
	}
	if got := CountWords(""); len(got) != 0 {
		t.Errorf("empty = %v, want empty", got)
	}
}
