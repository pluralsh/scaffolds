package core

import "context"

// Handler is a function's logic: it receives the decoded request and answers with a
// [Response].
type Handler[P, R any] interface {
	Handle(ctx context.Context, req Request[P]) (Response[R], error)
}

// HandlerFunc adapts a plain function to a [Handler].
type HandlerFunc[P, R any] func(ctx context.Context, req Request[P]) (Response[R], error)

// Handle calls f.
func (f HandlerFunc[P, R]) Handle(ctx context.Context, req Request[P]) (Response[R], error) {
	return f(ctx, req)
}
