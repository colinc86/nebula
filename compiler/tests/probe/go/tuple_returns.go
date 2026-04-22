package foo

import "io"

// qualified type in param and tuple return
func FindFile(tarFile io.Reader, filename string) (io.Reader, error) {
	return nil, nil
}

// multiple tuple return with error
func Lookup(key string) (int, string, error) {
	return 0, "", nil
}

// named returns
func Split(s string) (head string, tail string) {
	head = s
	tail = ""
	return
}
