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
	if n < 2 {
		return false
	}
	if n%2 == 0 {
		return n == 2
	}
	for i := 3; i*i <= n; i += 2 {
		if n%i == 0 {
			return false
		}
	}
	return true
}

// Fib returns the nth Fibonacci number with Fib(0)=0, Fib(1)=1.
// For n < 0 it returns 0.
func Fib(n int) int {
	// TODO: implement
	return 0
}
