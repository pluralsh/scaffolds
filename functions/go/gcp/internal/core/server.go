package core

import (
	"context"
	"errors"
	"io"
	"log/slog"
	"net/http"
)

// maxBodyBytes bounds the tool input a function reads.
const maxBodyBytes = 2 << 20

// Server serves a [Handler] over HTTP, e.g. as a Cloud Run function. Every POST is an
// invocation: the body is the workbench tool input and the response body is the [Response].
// Errors use the same errorType/errorMessage shape as Lambda function errors, with a 4xx/5xx
// status that callers report as failures.
type Server[P, R any] struct {
	handler Handler[P, R]
}

// NewServer returns a server invoking handler.
func NewServer[P, R any](handler Handler[P, R]) *Server[P, R] {
	return &Server[P, R]{handler: handler}
}

// ServeHTTP implements [http.Handler].
func (s *Server[P, R]) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		w.WriteHeader(http.StatusMethodNotAllowed)
		return
	}
	body, err := io.ReadAll(http.MaxBytesReader(w, r.Body, maxBodyBytes))
	if err != nil {
		if _, ok := errors.AsType[*http.MaxBytesError](err); ok {
			w.WriteHeader(http.StatusRequestEntityTooLarge)
			return
		}
		s.fail(r.Context(), w, InvalidRequestf("reading request body: %v", err))
		return
	}

	resp, err := s.invoke(r.Context(), body)
	if err != nil {
		s.fail(r.Context(), w, err)
		return
	}
	s.write(w, http.StatusOK, resp)
}

// invoke runs the handler on the tool input and encodes its response.
func (s *Server[P, R]) invoke(ctx context.Context, body []byte) ([]byte, error) {
	req, err := ParseRequest[P](body)
	if err != nil {
		return nil, err
	}
	resp, err := s.handler.Handle(ctx, req)
	if err != nil {
		return nil, err
	}
	out, err := canonicalJSON(resp)
	if err != nil {
		return nil, Providerf("%v", err)
	}
	return out, nil
}

// fail logs err and reports it to the caller. Errors other than [Error] are provider errors.
func (s *Server[P, R]) fail(ctx context.Context, w http.ResponseWriter, err error) {
	var e *Error
	if !errors.As(err, &e) {
		e = Providerf("%v", err)
	}
	slog.ErrorContext(ctx, "invocation failed", "error_type", string(e.Kind), "error", e.Error())
	body, err := canonicalJSON(errorBody{ErrorType: e.Kind, ErrorMessage: e.Error()})
	if err != nil {
		w.WriteHeader(e.Status())
		return
	}
	s.write(w, e.Status(), body)
}

func (*Server[P, R]) write(w http.ResponseWriter, status int, body []byte) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_, _ = w.Write(body)
}

// errorBody is the body of a failed invocation.
type errorBody struct {
	ErrorType    ErrorKind `json:"errorType"`
	ErrorMessage string    `json:"errorMessage"`
}
