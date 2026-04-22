package foo

type Level int

const (
	LevelDebug Level = iota
	LevelInfo
	LevelWarn
	LevelError
)

func (l Level) String() string {
	switch l {
	case LevelDebug:
		return "debug"
	case LevelInfo:
		return "info"
	case LevelWarn:
		return "warn"
	case LevelError:
		return "error"
	}
	return "?"
}

// bit-flag iota
type Perm uint

const (
	PermRead Perm = 1 << iota
	PermWrite
	PermExec
)
