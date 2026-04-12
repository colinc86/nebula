package taskpkg

// MergeStrings combines two maps; values in b override values in a on conflict.
func MergeStrings(a, b map[string]string) map[string]string {
	// TODO: implement
	return nil
}

// CountWords returns a map from each whitespace-separated word in s to its
// occurrence count. Empty input returns an empty map.
func CountWords(s string) map[string]int {
	out := map[string]int{}
	word := ""
	flush := func() {
		if word != "" {
			out[word]++
			word = ""
		}
	}
	for _, r := range s {
		if r == ' ' || r == '\t' || r == '\n' {
			flush()
		} else {
			word += string(r)
		}
	}
	flush()
	return out
}
