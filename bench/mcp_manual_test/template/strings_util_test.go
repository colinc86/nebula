package nebulatest

import "testing"

func TestReverse(t *testing.T) {
	cases := []struct{ in, want string }{
		{"", ""},
		{"a", "a"},
		{"abc", "cba"},
		{"hello", "olleh"},
		{"héllo", "olléh"},
	}
	for _, c := range cases {
		if got := Reverse(c.in); got != c.want {
			t.Errorf("Reverse(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

func TestTitleCase(t *testing.T) {
	cases := []struct{ in, want string }{
		{"", ""},
		{"hello", "Hello"},
		{"hello world", "Hello World"},
		{"HELLO WORLD", "Hello World"},
		{"  spaced  out  ", "  Spaced  Out  "},
	}
	for _, c := range cases {
		if got := TitleCase(c.in); got != c.want {
			t.Errorf("TitleCase(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

func TestCountRunes(t *testing.T) {
	cases := []struct {
		in   string
		want int
	}{
		{"", 0},
		{"abc", 3},
		{"héllo", 5},
		{"日本語", 3},
	}
	for _, c := range cases {
		if got := CountRunes(c.in); got != c.want {
			t.Errorf("CountRunes(%q) = %d, want %d", c.in, got, c.want)
		}
	}
}

func TestIsPalindrome(t *testing.T) {
	cases := []struct {
		in   string
		want bool
	}{
		{"", true},
		{"a", true},
		{"racecar", true},
		{"Racecar", true},
		{"A man a plan a canal Panama", true},
		{"hello", false},
	}
	for _, c := range cases {
		if got := IsPalindrome(c.in); got != c.want {
			t.Errorf("IsPalindrome(%q) = %v, want %v", c.in, got, c.want)
		}
	}
}
