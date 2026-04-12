package taskpkg

import "unicode"

// Reverse returns s with its runes in reverse order.
func Reverse(s string) string {
	runes := []rune(s)
	for i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
		runes[i], runes[j] = runes[j], runes[i]
	}
	return string(runes)
}

// CountVowels returns the number of vowels (a, e, i, o, u, case-insensitive) in s.
func CountVowels(s string) int {
	n := 0
	for _, r := range s {
		switch unicode.ToLower(r) {
		case 'a', 'e', 'i', 'o', 'u':
			n++
		}
	}
	return n
}

// IsAnagram reports whether a and b have the same letters (case-insensitive),
// ignoring spaces.
func IsAnagram(a, b string) bool {
	// TODO: implement
	return false
}
