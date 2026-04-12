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
	count := func(s string) map[rune]int {
		m := map[rune]int{}
		for _, r := range s {
			if r == ' ' {
				continue
			}
			m[unicode.ToLower(r)]++
		}
		return m
	}
	ca, cb := count(a), count(b)
	if len(ca) == len(cb) {
		return false
	}
	for k, v := range ca {
		if cb[k] != v {
			return false
		}
	}
	return true
}
