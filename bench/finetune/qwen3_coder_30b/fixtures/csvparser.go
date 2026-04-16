// Package nebulatest implements a simple RFC-4180-ish CSV parser with
// streaming record-at-a-time reads. It's a single-file educational
// implementation: lexing, field parsing, quoting/unquoting, and dialect
// configuration all live here.
//
// The public API is:
//
//	NewReader(r io.Reader) *Reader
//	NewReaderWithDialect(r io.Reader, d Dialect) *Reader
//	(r *Reader) Read() (Record, error)
//	(r *Reader) ReadAll() ([]Record, error)
//	FormatRecord(rec Record, d Dialect) string
//
// A Record is a []string of field values. Errors carry context about
// which line and column the parse failure occurred at.
package nebulatest

import (
	"bufio"
	"errors"
	"fmt"
	"io"
	"strings"
)

// ---------- Dialect ----------

// Dialect controls the CSV format: field separator, quote character,
// and whether to trim leading/trailing whitespace from unquoted fields.
type Dialect struct {
	Separator   rune // field separator (default ',')
	Quote       rune // quote character (default '"')
	TrimSpaces  bool // trim leading/trailing spaces from unquoted fields
	LazyQuotes  bool // allow bare quotes inside unquoted fields
}

// DefaultDialect returns the standard RFC 4180 dialect.
func DefaultDialect() Dialect {
	return Dialect{
		Separator:  ',',
		Quote:      '"',
		TrimSpaces: false,
		LazyQuotes: false,
	}
}

// TSVDialect returns a tab-separated-values dialect.
func TSVDialect() Dialect {
	return Dialect{
		Separator:  '\t',
		Quote:      '"',
		TrimSpaces: true,
		LazyQuotes: true,
	}
}

// ---------- Record ----------

// Record is one row of CSV data: a slice of field strings.
type Record = []string

// ---------- ParseError ----------

// ParseError reports a problem at a specific line and column.
type ParseError struct {
	Line   int
	Column int
	Msg    string
}

func (e *ParseError) Error() string {
	return fmt.Sprintf("csv: line %d col %d: %s", e.Line, e.Column, e.Msg)
}

// ---------- Reader ----------

// Reader reads records from a CSV-encoded stream.
type Reader struct {
	scanner *bufio.Scanner
	dialect Dialect
	line    int
}

// NewReader returns a Reader that uses the default RFC 4180 dialect.
func NewReader(r io.Reader) *Reader {
	return NewReaderWithDialect(r, DefaultDialect())
}

// NewReaderWithDialect returns a Reader configured with the given Dialect.
func NewReaderWithDialect(r io.Reader, d Dialect) *Reader {
	return &Reader{
		scanner: bufio.NewScanner(r),
		dialect: d,
		line:    0,
	}
}

// Read returns the next record (row) from the input. Returns io.EOF when
// the input is exhausted. Blank lines are skipped.
func (r *Reader) Read() (Record, error) {
	for r.scanner.Scan() {
		r.line++
		raw := r.scanner.Text()
		if len(strings.TrimSpace(raw)) == 0 {
			continue // skip blank lines
		}
		return r.parseLine(raw)
	}
	if err := r.scanner.Err(); err != nil {
		return nil, err
	}
	return nil, io.EOF
}

// ReadAll reads all remaining records and returns them as a slice.
func (r *Reader) ReadAll() ([]Record, error) {
	var records []Record
	for {
		rec, err := r.Read()
		if errors.Is(err, io.EOF) {
			return records, nil
		}
		if err != nil {
			return records, err
		}
		records = append(records, rec)
	}
}

// parseLine splits a single CSV line into fields, respecting quoting.
func (r *Reader) parseLine(line string) (Record, error) {
	var fields []string
	col := 0
	for col < len(line) {
		if line[col] == byte(r.dialect.Quote) {
			field, newCol, err := r.parseQuotedField(line, col)
			if err != nil {
				return nil, err
			}
			fields = append(fields, field)
			col = newCol
			// Skip the separator after the quoted field
			if col < len(line) && line[col] == byte(r.dialect.Separator) {
				col++
			}
		} else {
			field, newCol := r.parseUnquotedField(line, col)
			if r.dialect.TrimSpaces {
				field = strings.TrimSpace(field)
			}
			fields = append(fields, field)
			col = newCol
			if col < len(line) && line[col] == byte(r.dialect.Separator) {
				col++
			}
		}
	}
	// Trailing separator means one more empty field
	if len(line) > 0 && line[len(line)-1] == byte(r.dialect.Separator) {
		fields = append(fields, "")
	}
	return fields, nil
}

// parseQuotedField reads a field that starts with a quote character.
// Returns the unescaped field value, the position after the closing quote
// (or after the separator), and any error.
func (r *Reader) parseQuotedField(line string, start int) (string, int, error) {
	q := byte(r.dialect.Quote)
	i := start + 1 // skip opening quote
	var buf strings.Builder
	for i < len(line) {
		ch := line[i]
		if ch == q {
			// Two consecutive quotes → escaped quote
			if i+1 < len(line) && line[i+1] == q {
				buf.WriteByte(q)
				i += 2
				continue
			}
			// Single quote → end of field
			return buf.String(), i + 1, nil
		}
		buf.WriteByte(ch)
		i++
	}
	return "", start, &ParseError{
		Line:   r.line,
		Column: start + 1,
		Msg:    "unterminated quoted field",
	}
}

// parseUnquotedField reads a field up to the next separator or end of line.
func (r *Reader) parseUnquotedField(line string, start int) (string, int) {
	sep := byte(r.dialect.Separator)
	i := start
	for i < len(line) && line[i] != sep {
		i++
	}
	return line[start:i], i
}

// ---------- Formatting ----------

// FormatRecord formats a single record as a CSV line using the given dialect.
// Fields containing the separator, the quote character, or a newline are
// quoted. The result does NOT include a trailing newline.
func FormatRecord(rec Record, d Dialect) string {
	sep := string(d.Separator)
	q := string(d.Quote)
	var parts []string
	for _, field := range rec {
		needsQuote := strings.ContainsRune(field, d.Separator) ||
			strings.ContainsRune(field, d.Quote) ||
			strings.ContainsAny(field, "\r\n")
		if needsQuote {
			escaped := strings.ReplaceAll(field, q, q+q)
			parts = append(parts, q+escaped+q)
		} else {
			parts = append(parts, field)
		}
	}
	return strings.Join(parts, sep)
}

// ---------- Utilities ----------

// CountFields returns the number of fields in a CSV line without fully
// parsing it. Useful for quick validation. Returns -1 on parse error.
func CountFields(line string, d Dialect) int {
	r := NewReaderWithDialect(strings.NewReader(line), d)
	rec, err := r.Read()
	if err != nil {
		return -1
	}
	return len(rec)
}

// HasHeader peeks at the first line and reports whether all fields look
// like header names (non-numeric, non-empty). Does not consume the reader.
func HasHeader(firstLine string, d Dialect) bool {
	r := NewReaderWithDialect(strings.NewReader(firstLine), d)
	rec, err := r.Read()
	if err != nil || len(rec) == 0 {
		return false
	}
	for _, field := range rec {
		trimmed := strings.TrimSpace(field)
		if trimmed == "" {
			return false
		}
		// If the field parses as a number, it's probably data, not a header
		allDigits := true
		for _, ch := range trimmed {
			if ch < '0' || ch > '9' {
				allDigits = false
				break
			}
		}
		if allDigits {
			return false
		}
	}
	return true
}

// SplitQuick is a fast-path splitter for lines known to contain no quoted
// fields. It splits on the separator and optionally trims whitespace.
func SplitQuick(line string, d Dialect) []string {
	parts := strings.Split(line, string(d.Separator))
	if d.TrimSpaces {
		for i, p := range parts {
			parts[i] = strings.TrimSpace(p)
		}
	}
	return parts
}

// JoinFields joins fields with the dialect's separator. No quoting is applied.
func JoinFields(fields []string, d Dialect) string {
	return strings.Join(fields, string(d.Separator))
}

// FilterRecords returns only the records for which keep returns true.
func FilterRecords(records []Record, keep func(Record) bool) []Record {
	var out []Record
	for _, rec := range records {
		if keep(rec) {
			out = append(out, rec)
		}
	}
	return out
}

// MapField applies fn to the field at the given column index in each record.
// Returns a new slice of records; the originals are not modified.
func MapField(records []Record, col int, fn func(string) string) []Record {
	out := make([]Record, len(records))
	for i, rec := range records {
		row := make(Record, len(rec))
		copy(row, rec)
		if col >= 0 && col < len(row) {
			row[col] = fn(row[col])
		}
		out[i] = row
	}
	return out
}
