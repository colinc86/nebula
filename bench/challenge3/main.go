package main

import (
	"fmt"
	"strings"
	"unicode"
)

// --- CSV domain ---

type Row struct {
	Fields []string
}

type Table struct {
	Headers []string
	Rows    []Row
}

func NewTable(headers ...string) *Table {
	return &Table{
		Headers: headers,
		Rows:    []Row{},
	}
}

func (t *Table) AddRow(fields ...string) {
	t.Rows = append(t.Rows, Row{Fields: fields})
}

func (t *Table) RowCount() int {
	return len(t.Rows)
}

func (t *Table) ColCount() int {
	return len(t.Headers)
}

// --- Simple tokenizer ---

func Tokenize(input string) []string {
	var tokens []string
	var current strings.Builder

	for _, r := range input {
		if unicode.IsSpace(r) {
			if current.Len() > 0 {
				tokens = append(tokens, current.String())
				current.Reset()
			}
		} else {
			current.WriteRune(r)
		}
	}
	if current.Len() > 0 {
		tokens = append(tokens, current.String())
	}
	return tokens
}

// --- String utilities ---

func Reverse(s string) string {
	runes := []rune(s)
	for i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
		runes[i], runes[j] = runes[j], runes[i]
	}
	return string(runes)
}

func CountWords(s string) int {
	return len(Tokenize(s))
}

func CountOccurrences(s, substr string) int {
	if substr == "" {
		return 0
	}
	count := 0
	index := 0
	for {
		found := strings.Index(s[index:], substr)
		if found == -1 {
			return count
		}
		count++
		index += found + len(substr)
	}
}

// --- Line utilities ---

func SplitLines(s string) []string {
	return strings.Split(strings.ReplaceAll(s, "\r\n", "\n"), "\n")
}

func NonEmptyLines(s string) []string {
	var result []string
	for _, line := range SplitLines(s) {
		if strings.TrimSpace(line) != "" {
			result = append(result, line)
		}
	}
	return result
}

// --- Stubs for challenge features ---

// ParseCSV should parse a CSV string into a Table. The first line is the
// header row. Fields are comma-separated. Fields that contain a comma
// must be quoted with double-quotes; escaped quotes inside a quoted field
// are represented as "" (two consecutive double-quotes).
func ParseCSV(input string) (*Table, error) {
	panic("CHALLENGE: implement ParseCSV")
}

// WordFrequency should return a map from each word to its occurrence count,
// ignoring case. Words are separated by whitespace or punctuation.
// Punctuation should be stripped from words before counting.
func WordFrequency(text string) map[string]int {
	panic("CHALLENGE: implement WordFrequency")
}

// FormatTable should format a Table as aligned text with column separators.
// Each column should be padded to the width of its widest value.
// The format is:
//
//	Header1 | Header2 | Header3
//	--------|---------|--------
//	val1    | val2    | val3
func FormatTable(t *Table) string {
	panic("CHALLENGE: implement FormatTable")
}

// FindAllMatches should return the start indices of all non-overlapping
// occurrences of pattern in text. It supports two wildcards:
//   - '?' matches any single character
//   - '*' matches zero or more characters (greedy)
//
// Example: pattern "f?o" matches "foo", "fao" but not "fo".
// Example: pattern "a*b" matches "ab", "acb", "axyzb".
func FindAllMatches(text, pattern string) []int {
	panic("CHALLENGE: implement FindAllMatches")
}

// --- Main ---

func main() {
	csv := "name,age,city\nAlice,30,NYC\nBob,25,SF\n"
	fmt.Println("Sample CSV:")
	fmt.Println(csv)

	text := "The quick brown fox jumps over the lazy dog. The dog barks!"
	tokens := Tokenize(text)
	fmt.Printf("Tokens: %v\n", tokens)
	fmt.Printf("Word count: %d\n", CountWords(text))
	fmt.Printf("Occurrences of 'the': %d\n", CountOccurrences(strings.ToLower(text), "the"))

	reversed := Reverse("hello")
	fmt.Printf("Reversed: %s\n", reversed)
}
