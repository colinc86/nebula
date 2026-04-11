package main

import (
	"testing"
)

// --- Existing tests ---

func TestTokenize(t *testing.T) {
	tokens := Tokenize("hello world foo")
	if len(tokens) != 3 {
		t.Fatalf("expected 3 tokens, got %d", len(tokens))
	}
	if tokens[0] != "hello" {
		t.Fatalf("unexpected first token: %q", tokens[0])
	}
}

func TestReverse(t *testing.T) {
	if Reverse("hello") != "olleh" {
		t.Fatal("Reverse failed")
	}
	if Reverse("") != "" {
		t.Fatal("Reverse empty string failed")
	}
}

func TestCountWords(t *testing.T) {
	if CountWords("one two three") != 3 {
		t.Fatal("CountWords failed")
	}
}

func TestCountOccurrences(t *testing.T) {
	if CountOccurrences("abcabcabc", "abc") != 3 {
		t.Fatal("CountOccurrences failed")
	}
	if CountOccurrences("hello", "z") != 0 {
		t.Fatal("CountOccurrences with no match failed")
	}
}

func TestSplitLines(t *testing.T) {
	lines := SplitLines("a\nb\r\nc")
	if len(lines) != 3 {
		t.Fatalf("expected 3 lines, got %d", len(lines))
	}
}

func TestNonEmptyLines(t *testing.T) {
	lines := NonEmptyLines("a\n\nb\n  \nc")
	if len(lines) != 3 {
		t.Fatalf("expected 3 non-empty lines, got %d", len(lines))
	}
}

func TestTableBasic(t *testing.T) {
	table := NewTable("name", "age")
	table.AddRow("Alice", "30")
	table.AddRow("Bob", "25")
	if table.RowCount() != 2 || table.ColCount() != 2 {
		t.Fatal("Table counts wrong")
	}
}

// --- CHALLENGE TESTS ---

func TestChallenge_ParseCSV(t *testing.T) {
	t.Skip("CHALLENGE: Implement ParseCSV")

	input := `name,age,city
Alice,30,NYC
Bob,25,"San Francisco"
Carol,35,"Los, Angeles"`

	table, err := ParseCSV(input)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if table.ColCount() != 3 {
		t.Fatalf("expected 3 columns, got %d", table.ColCount())
	}
	if table.RowCount() != 3 {
		t.Fatalf("expected 3 rows, got %d", table.RowCount())
	}
	if table.Rows[0].Fields[0] != "Alice" {
		t.Fatal("first cell wrong")
	}
	if table.Rows[2].Fields[2] != "Los, Angeles" {
		t.Fatalf("quoted field wrong: got %q", table.Rows[2].Fields[2])
	}
}

func TestChallenge_WordFrequency(t *testing.T) {
	t.Skip("CHALLENGE: Implement WordFrequency")

	text := "The cat sat on the mat. The cat was happy!"
	freq := WordFrequency(text)
	if freq["the"] != 3 {
		t.Fatalf("expected 'the' count 3, got %d", freq["the"])
	}
	if freq["cat"] != 2 {
		t.Fatalf("expected 'cat' count 2, got %d", freq["cat"])
	}
	if freq["happy"] != 1 {
		t.Fatalf("expected 'happy' count 1, got %d", freq["happy"])
	}
}

func TestChallenge_FormatTable(t *testing.T) {
	t.Skip("CHALLENGE: Implement FormatTable")

	table := NewTable("Name", "Age")
	table.AddRow("Alice", "30")
	table.AddRow("Bob", "25")

	result := FormatTable(table)
	if len(result) == 0 {
		t.Fatal("FormatTable returned empty string")
	}
	// Must contain headers
	if !contains(result, "Name") || !contains(result, "Age") {
		t.Fatal("FormatTable missing headers")
	}
	// Must contain separator
	if !contains(result, "|") {
		t.Fatal("FormatTable missing column separator")
	}
	// Must contain all data
	if !contains(result, "Alice") || !contains(result, "Bob") {
		t.Fatal("FormatTable missing row data")
	}
}

func TestChallenge_FindAllMatches(t *testing.T) {
	t.Skip("CHALLENGE: Implement FindAllMatches")

	// Simple literal
	matches := FindAllMatches("abcdefabc", "abc")
	if len(matches) != 2 || matches[0] != 0 || matches[1] != 6 {
		t.Fatalf("literal match failed: %v", matches)
	}

	// Single-char wildcard
	matches = FindAllMatches("foo bar foo baz", "f?o")
	if len(matches) != 2 {
		t.Fatalf("? wildcard failed: %v", matches)
	}

	// Multi-char wildcard
	matches = FindAllMatches("axyzb", "a*b")
	if len(matches) != 1 || matches[0] != 0 {
		t.Fatalf("* wildcard failed: %v", matches)
	}
}

// Helper
func contains(s, substr string) bool {
	return len(s) >= len(substr) && CountOccurrences(s, substr) > 0
}
