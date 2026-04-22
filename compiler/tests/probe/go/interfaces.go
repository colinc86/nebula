package foo

type Reader interface {
	Read(p []byte) (int, error)
}

type Closer interface {
	Close() error
}

type ReadCloser interface {
	Reader
	Closer
}

// struct embedding
type MyReader struct {
	Reader
	name string
}
