package gke

import (
	"context"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gcp"
)

// EndpointVar overrides the GKE API endpoint, e.g. for local tests. It is the host only,
// without the /v1 path, e.g. http://127.0.0.1:8080/.
const EndpointVar = "GCP_CONTAINER_ENDPOINT"

// EnvConnector connects to GKE as the running service, in the project named by
// [gcp.ProjectVar]. It creates the client on first use and reuses it for later invocations.
type EnvConnector struct {
	client *gcp.Lazy[*apiClient]
}

// NewEnvConnector returns a connector that has not connected yet.
func NewEnvConnector() *EnvConnector {
	return &EnvConnector{client: gcp.NewLazy(func(ctx context.Context, project string) (*apiClient, error) {
		return newAPIClient(ctx, project, gcp.EndpointOptions(EndpointVar)...)
	})}
}

// Connect implements [Connector].
func (c *EnvConnector) Connect(ctx context.Context) (Client, error) {
	return c.client.Get(ctx)
}
