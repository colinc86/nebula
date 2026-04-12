package kvproj

import (
	"errors"
	"unicode/utf8"
)

// Validator enforces simple shape rules on KVCache keys and values.
type Validator struct {
	maxKeyLen   int
	maxValueLen int
}

// NewValidator constructs a Validator with the given length limits.
func NewValidator(maxKeyLen, maxValueLen int) *Validator {
	return &Validator{maxKeyLen: maxKeyLen, maxValueLen: maxValueLen}
}

// Key validates a single KVCache key.
func (v *Validator) Key(key string) error {
	if key == "" {
		return errors.New("key must not be empty")
	}
	if utf8.RuneCountInString(key) > v.maxKeyLen {
		return errors.New("key too long")
	}
	return nil
}

// KeyValue validates a key/value pair before insertion into the KVCache.
func (v *Validator) KeyValue(key, value string) error {
	if err := v.Key(key); err != nil {
		return err
	}
	if utf8.RuneCountInString(value) > v.maxValueLen {
		return errors.New("value too long")
	}
	return nil
}
