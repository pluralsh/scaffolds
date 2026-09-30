package core

import (
	"fmt"
	"net/http"
)

// ErrorKind is the stable, machine-readable error category reported to the caller as the
// error type.
type ErrorKind string

const (
	// KindInvalidRequest is a request the function can't act on.
	KindInvalidRequest ErrorKind = "InvalidRequest"
	// KindProvider is a failure of the cloud provider or of the function's own setup.
	KindProvider ErrorKind = "Provider"
)

// Error is a failure that prevents a function from producing a [Response].
//
// Guard failures are not errors: they are reported as [OutcomeRefused] so the caller can see
// why an action was not taken.
type Error struct {
	Kind    ErrorKind
	Message string
}

// InvalidRequestf reports a request the function can't act on.
func InvalidRequestf(format string, args ...any) *Error {
	return &Error{Kind: KindInvalidRequest, Message: fmt.Sprintf(format, args...)}
}

// Providerf reports a failure of the cloud provider or of the function's own setup.
func Providerf(format string, args ...any) *Error {
	return &Error{Kind: KindProvider, Message: fmt.Sprintf(format, args...)}
}

// Error implements error.
func (e *Error) Error() string {
	if e.Kind == KindInvalidRequest {
		return "invalid request: " + e.Message
	}
	return "cloud provider error: " + e.Message
}

// Status is the HTTP status callers see for the error.
func (e *Error) Status() int {
	if e.Kind == KindInvalidRequest {
		return http.StatusBadRequest
	}
	return http.StatusBadGateway
}
