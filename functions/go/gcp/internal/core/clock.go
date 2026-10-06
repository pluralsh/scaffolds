package core

import "time"

// Clock tells the current time in Unix seconds.
type Clock interface {
	Now() int64
}

// SystemClock is the wall clock.
type SystemClock struct{}

// Now implements [Clock].
func (SystemClock) Now() int64 {
	return time.Now().Unix()
}
