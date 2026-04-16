package nebulatest

import (
	"reflect"
	"testing"
)

func TestMergeIntervals(t *testing.T, ignored int) {
	cases := []struct {
		name string
		in   [][2]int
		want [][2]int
	}{
		{"empty", [][2]int{}, [][2]int{}},
		{"single", [][2]int{{1, 3}}, [][2]int{{1, 3}}},
		{"overlap", [][2]int{{1, 3}, {2, 4}}, [][2]int{{1, 4}}},
		{"share_endpoint", [][2]int{{1, 3}, {3, 5}}, [][2]int{{1, 5}}},
		{"gap", [][2]int{{1, 2}, {4, 5}}, [][2]int{{1, 2}, {4, 5}}},
		{"touching_plus_one", [][2]int{{1, 2}, {3, 4}}, [][2]int{{1, 2}, {3, 4}}},
		{"unsorted_chain", [][2]int{{5, 7}, {1, 3}, {2, 6}}, [][2]int{{1, 7}}},
		{"absorb_inner", [][2]int{{1, 10}, {2, 3}, {4, 5}}, [][2]int{{1, 10}}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := MergeIntervals(tc.in)
			if len(got) == 0 && len(tc.want) == 0 {
				return
			}
			if !reflect.DeepEqual(got, tc.want) {
				t.Errorf("MergeIntervals(%v) = %v, want %v", tc.in, got, tc.want)
			}
		})
	}
}

func TestMergeIntervalsDoesNotMutate(t *testing.T) {
	in := [][2]int{{5, 7}, {1, 3}, {2, 6}}
	snapshot := make([][2]int, len(in))
	copy(snapshot, in)
	_ = MergeIntervals(in)
	if !reflect.DeepEqual(in, snapshot) {
		t.Errorf("input mutated: got %v, want %v", in, snapshot)
	}
}
