package foo

import (
	"errors"
	"fmt"
)

var ErrBadInput = errors.New("bad input")

// panic/recover
func safe(fn func()) (err error) {
	defer func() {
		if r := recover(); r != nil {
			err = fmt.Errorf("recovered: %v", r)
		}
	}()
	fn()
	return nil
}

// error wrapping
func parseNum(s string) (int, error) {
	if s == "" {
		return 0, fmt.Errorf("%w: empty", ErrBadInput)
	}
	return 0, nil
}

// error check pattern
func process(s string) error {
	n, err := parseNum(s)
	if err != nil {
		return fmt.Errorf("process: %w", err)
	}
	_ = n
	return nil
}
