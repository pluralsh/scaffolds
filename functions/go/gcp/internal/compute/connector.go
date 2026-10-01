package compute

import (
	"context"
	"os"
	"strings"
	"sync"

	"google.golang.org/api/option"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

const (
	// ProjectVar names the project the service runs in. Cloud Run sets it and the Google
	// client libraries read it.
	ProjectVar = "GOOGLE_CLOUD_PROJECT"

	// EndpointVar overrides the Compute Engine API endpoint, e.g. for local tests. It is the
	// host only, without the /compute/v1 path, e.g. http://127.0.0.1:8080.
	EndpointVar = "GCP_COMPUTE_ENDPOINT"
)

// EnvConnector connects to Compute Engine as the running service, in the project named by
// [ProjectVar]. It creates the client on first use and reuses it for later invocations. If
// creating it fails, the next call tries again.
type EnvConnector struct {
	mu     sync.Mutex
	client *sdkClient
}

// NewEnvConnector returns a connector that has not connected yet.
func NewEnvConnector() *EnvConnector {
	return &EnvConnector{}
}

// Connect implements [Connector].
func (c *EnvConnector) Connect(ctx context.Context) (Client, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.client != nil {
		return c.client, nil
	}

	project := strings.TrimSpace(os.Getenv(ProjectVar))
	if project == "" {
		return nil, core.Providerf("%s is not set", ProjectVar)
	}
	var opts []option.ClientOption
	if endpoint := os.Getenv(EndpointVar); endpoint != "" {
		opts = append(opts, option.WithEndpoint(endpoint))
	}
	// The client outlives the invocation that creates it.
	client, err := newSDKClient(context.WithoutCancel(ctx), project, opts...)
	if err != nil {
		return nil, err
	}
	c.client = client
	return client, nil
}
