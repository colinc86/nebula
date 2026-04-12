package taskpkg

import "testing"

func TestReverse(t *testing.T) {
	cases := []struct{ in, want string }{{"", ""}, {"a", "a"}, {"abc", "cba"}, {"hello", "olleh"}}
	for _, c := range cases {
		if got := Reverse(c.in); got != c.want {
			t.Errorf("Reverse(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

func TestCountVowels(t *testing.T) {
	cases := []struct {
		in   string
		want int
	}{{"", 0}, {"hello", 2}, {"AEIOU", 5}, {"xyz", 0}, {"Programming", 3}}
	for _, c := range cases {
		if got := CountVowels(c.in); got != c.want {
			t.Errorf("CountVowels(%q) = %d, want %d", c.in, got, c.want)
		}
	}
}

func TestIsAnagram(t *testing.T) {
	cases := []struct {
		a, b string
		want bool
	}{
		{"listen", "silent", true},
		{"hello", "world", false},
		{"Astronomer", "Moon starer", true},
		{"a", "ab", false},
	}
	for _, c := range cases {
		if got := IsAnagram(c.a, c.b); got != c.want {
			t.Errorf("IsAnagram(%q,%q) = %v want %v", c.a, c.b, got, c.want)
		}
	}
}
