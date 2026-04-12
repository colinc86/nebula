package taskpkg

// MergeStrings combines two maps; values in b override values in a on conflict.
func MergeStrings(a, b map[string]string) map[string]string {
	out := make(map[string]string, len(a)+len(b))
	for k, v := range a {
		out[k] = v
	}
	for k, v := range b {
		out[k] = v
	}
	return out
}

// CountWords returns a map from each whitespace-separated word in s to its
// occurrence count. Empty input returns an empty map.
func CountWords(s string) map[string]int {
	// TODO: implement
	return nil
}
