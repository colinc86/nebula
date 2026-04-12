package taskpkg

// Gcd returns the greatest common divisor of a and b using Euclid's algorithm.
// Both inputs are taken as absolute values.
func Gcd(a, b int) int {
	if a < 0 {
		a = -a
	}
	if b < 0 {
		b = -b
	}
	for b != 0 {
		a, b = b, a%b
	}
	return a
}

// IsPrime reports whether n is prime. n < 2 is not prime.
func IsPrime(n int) bool {
	// TODO: implement
	return false
}

// Fib returns the nth Fibonacci number with Fib(0)=0, Fib(1)=1.
// For n < 0 it returns 0.
func Fib(n int) int {
	if n < 0 {
		return 0
	}
	a, b := 0, 1
	for i := 0; i < n; i++ {
		a, b = b, a+b
	}
	return a
}
