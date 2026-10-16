// Package gcp holds what the Google Cloud API clients of the functions share: the project they
// act in, endpoint overrides for local tests, and clients created once and reused.
package gcp

import (
	"context"
	"errors"
	"net/http"
	"os"
	"strings"
	"sync"

	"google.golang.org/api/googleapi"
	"google.golang.org/api/option"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// ProjectVar names the project the service runs in. Cloud Run sets it and the Google client
// libraries read it.
const ProjectVar = "GOOGLE_CLOUD_PROJECT"

// Project is the project named by [ProjectVar].
func Project() (string, error) {
	project := strings.TrimSpace(os.Getenv(ProjectVar))
	if project == "" {
		return "", core.Providerf("%s is not set", ProjectVar)
	}
	return project, nil
}

// EndpointOptions points a client at the endpoint in the environment variable endpointVar, if
// it is set, e.g. a fake server in local tests.
func EndpointOptions(endpointVar string) []option.ClientOption {
	if endpoint := os.Getenv(endpointVar); endpoint != "" {
		return []option.ClientOption{option.WithEndpoint(endpoint)}
	}
	return nil
}

// Lazy creates a client for the project on first use and reuses it for later invocations. If
// creating it fails, the next call tries again.
type Lazy[T any] struct {
	create func(ctx context.Context, project string) (T, error)

	mu     sync.Mutex
	client T
	ready  bool
}

// NewLazy returns a Lazy that creates its client with create.
func NewLazy[T any](create func(ctx context.Context, project string) (T, error)) *Lazy[T] {
	return &Lazy[T]{create: create}
}

// Get returns the client, creating it first if needed.
func (l *Lazy[T]) Get(ctx context.Context) (T, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.ready {
		return l.client, nil
	}

	var zero T
	project, err := Project()
	if err != nil {
		return zero, err
	}
	// The client outlives the invocation that creates it.
	client, err := l.create(context.WithoutCancel(ctx), project)
	if err != nil {
		return zero, err
	}
	l.client, l.ready = client, true
	return client, nil
}

// NotFound reports whether err is an API's answer that the resource doesn't exist.
func NotFound(err error) bool {
	apiErr, ok := errors.AsType[*googleapi.Error](err)
	return ok && apiErr.Code == http.StatusNotFound
}
