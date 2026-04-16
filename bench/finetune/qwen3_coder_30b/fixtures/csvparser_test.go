package nebulatest

import (
	"io"
	"strings"
	"testing"
)

func TestReadSimple(t *testing.T) {
	input := "a,b,c\n1,2,3\n"
	r := NewReader(strings.NewReader(input))
	rec, err := r.Read()
	if err != nil {
		t.Fatal(err)
	}
	if len(rec) != 3 || rec[0] != "a" || rec[1] != "b" || rec[2] != "c" {
		t.Errorf("row 1 = %v, want [a b c]", rec)
	}
	rec, err = r.Read()
	if err != nil {
		t.Fatal(err)
	}
	if len(rec) != 3 || rec[0] != "1" || rec[1] != "2" || rec[2] != "3" {
		t.Errorf("row 2 = %v, want [1 2 3]", rec)
	}
	_, err = r.Read()
	if err != io.EOF {
		t.Errorf("expected EOF, got %v", err)
	}
}

func TestReadQuoted(t *testing.T) {
	input := `"hello, world",unquoted,"has ""escaped"" quotes"` + "\n"
	r := NewReader(strings.NewReader(input))
	rec, err := r.Read()
	if err != nil {
		t.Fatal(err)
	}
	want := []string{"hello, world", "unquoted", `has "escaped" quotes`}
	for i, w := range want {
		if i >= len(rec) || rec[i] != w {
			t.Errorf("field %d = %q, want %q", i, rec[i], w)
		}
	}
}

func TestReadAll(t *testing.T) {
	input := "a,b\n1,2\n3,4\n"
	r := NewReader(strings.NewReader(input))
	recs, err := r.ReadAll()
	if err != nil {
		t.Fatal(err)
	}
	if len(recs) != 3 {
		t.Fatalf("got %d records, want 3", len(recs))
	}
}

func TestSkipBlankLines(t *testing.T) {
	input := "a,b\n\n\n1,2\n"
	r := NewReader(strings.NewReader(input))
	recs, err := r.ReadAll()
	if err != nil {
		t.Fatal(err)
	}
	if len(recs) != 2 {
		t.Fatalf("got %d records, want 2 (blank lines should be skipped)", len(recs))
	}
}

func TestTSVDialect(t *testing.T) {
	input := "name\tage\tcity\n  Alice \t 30 \t  NYC \n"
	r := NewReaderWithDialect(strings.NewReader(input), TSVDialect())
	recs, err := r.ReadAll()
	if err != nil {
		t.Fatal(err)
	}
	if len(recs) != 2 {
		t.Fatalf("got %d records, want 2", len(recs))
	}
	// TSV with TrimSpaces: leading/trailing spaces should be removed
	if recs[1][0] != "Alice" {
		t.Errorf("field 0 = %q, want %q (trimmed)", recs[1][0], "Alice")
	}
}

func TestUnterminatedQuote(t *testing.T) {
	input := `"unterminated` + "\n"
	r := NewReader(strings.NewReader(input))
	_, err := r.Read()
	if err == nil {
		t.Fatal("expected error for unterminated quote")
	}
	pe, ok := err.(*ParseError)
	if !ok {
		t.Fatalf("expected *ParseError, got %T", err)
	}
	if pe.Line != 1 {
		t.Errorf("ParseError.Line = %d, want 1", pe.Line)
	}
}

func TestFormatRecord(t *testing.T) {
	rec := Record{"hello", "world, earth", `has "quotes"`}
	got := FormatRecord(rec, DefaultDialect())
	want := `hello,"world, earth","has ""quotes"""`
	if got != want {
		t.Errorf("FormatRecord = %q, want %q", got, want)
	}
}

func TestCountFields(t *testing.T) {
	n := CountFields("a,b,c,d", DefaultDialect())
	if n != 4 {
		t.Errorf("CountFields = %d, want 4", n)
	}
}

func TestHasHeader(t *testing.T) {
	if !HasHeader("name,age,city", DefaultDialect()) {
		t.Error("expected HasHeader=true for text headers")
	}
	if HasHeader("1,2,3", DefaultDialect()) {
		t.Error("expected HasHeader=false for numeric fields")
	}
}

func TestSplitQuick(t *testing.T) {
	got := SplitQuick("a,b,c", DefaultDialect())
	if len(got) != 3 || got[0] != "a" || got[2] != "c" {
		t.Errorf("SplitQuick = %v", got)
	}
}

func TestFilterRecords(t *testing.T) {
	recs := []Record{{"a", "1"}, {"b", "2"}, {"c", "3"}}
	filtered := FilterRecords(recs, func(r Record) bool { return r[1] != "2" })
	if len(filtered) != 2 {
		t.Errorf("filtered %d records, want 2", len(filtered))
	}
}

func TestMapField(t *testing.T) {
	recs := []Record{{"hello", "world"}, {"foo", "bar"}}
	mapped := MapField(recs, 0, strings.ToUpper)
	if mapped[0][0] != "HELLO" || mapped[1][0] != "FOO" {
		t.Errorf("MapField = %v", mapped)
	}
	// Original should be unmodified
	if recs[0][0] != "hello" {
		t.Error("original was modified")
	}
}

func TestTrailingSeparator(t *testing.T) {
	input := "a,b,\n"
	r := NewReader(strings.NewReader(input))
	rec, err := r.Read()
	if err != nil {
		t.Fatal(err)
	}
	if len(rec) != 3 || rec[2] != "" {
		t.Errorf("trailing separator: got %v (len %d), want [a b ] (len 3)", rec, len(rec))
	}
}
