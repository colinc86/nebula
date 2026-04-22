package foo

import "fmt"

func greet(name string, age int) string {
	return fmt.Sprintf("hi %s, you are %d", name, age)
}

func fmtFloat(x float64) string {
	return fmt.Sprintf("%.2f", x)
}

func debug(kv map[string]int) string {
	return fmt.Sprintf("%+v", kv)
}

func logErr(err error) {
	fmt.Fprintf(nil, "err: %v", err)
}

// Stringer interface
type Point struct{ X, Y int }

func (p Point) String() string {
	return fmt.Sprintf("(%d,%d)", p.X, p.Y)
}
