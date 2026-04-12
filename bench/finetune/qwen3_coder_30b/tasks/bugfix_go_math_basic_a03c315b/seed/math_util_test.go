package taskpkg

import "testing"

func TestGcd(t *testing.T) {
	cases := []struct{ a, b, want int }{{12, 8, 4}, {17, 5, 1}, {0, 5, 5}, {-12, 8, 4}, {100, 75, 25}}
	for _, c := range cases {
		if got := Gcd(c.a, c.b); got != c.want {
			t.Errorf("Gcd(%d,%d) = %d want %d", c.a, c.b, got, c.want)
		}
	}
}

func TestIsPrime(t *testing.T) {
	cases := []struct {
		n    int
		want bool
	}{{0, false}, {1, false}, {2, true}, {3, true}, {4, false}, {17, true}, {100, false}, {97, true}}
	for _, c := range cases {
		if got := IsPrime(c.n); got != c.want {
			t.Errorf("IsPrime(%d) = %v want %v", c.n, got, c.want)
		}
	}
}

func TestFib(t *testing.T) {
	cases := []struct{ n, want int }{{0, 0}, {1, 1}, {2, 1}, {3, 2}, {4, 3}, {5, 5}, {10, 55}, {-3, 0}}
	for _, c := range cases {
		if got := Fib(c.n); got != c.want {
			t.Errorf("Fib(%d) = %d want %d", c.n, got, c.want)
		}
	}
}
