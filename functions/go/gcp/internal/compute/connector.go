package compute

import (
	"context"
	"os"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gcp"
)

// EndpointVar overrides the Compute Engine API endpoint, e.g. for local tests. It is the host
// only, without the /compute/v1 path, e.g. http://127.0.0.1:8080.
const EndpointVar = "GCP_COMPUTE_ENDPOINT"

// EnvConnector connects to Compute Engine as the running service, in the project named by
// [gcp.ProjectVar]. It creates the client on first use and reuses it for later invocations. If
// creating it fails, the next call tries again.
type EnvConnector struct {
	client *gcp.Lazy[*sdkClient]
}

// NewEnvConnector returns a connector that has not connected yet.
func NewEnvConnector() *EnvConnector {
	return &EnvConnector{client: gcp.NewLazy(func(ctx context.Context, project string) (*sdkClient, error) {
		return newSDKClient(ctx, project, os.Getenv(EndpointVar))
	})}
}

// Connect implements [Connector].
func (c *EnvConnector) Connect(ctx context.Context) (Client, error) {
	return c.client.Get(ctx)
}
