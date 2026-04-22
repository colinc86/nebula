package foo

import "fmt"

// type switch
func describe(x interface{}) string {
	switch v := x.(type) {
	case int:
		return fmt.Sprintf("int %d", v)
	case string:
		return fmt.Sprintf("str %q", v)
	case []int:
		return fmt.Sprintf("ints len=%d", len(v))
	case nil:
		return "nil"
	default:
		return fmt.Sprintf("other %T", v)
	}
}

// value switch
func weekday(d int) string {
	switch d {
	case 0, 6:
		return "weekend"
	case 1, 2, 3, 4, 5:
		return "weekday"
	default:
		return "?"
	}
}
