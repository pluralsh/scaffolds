package gke

import (
	"context"
	"fmt"

	container "google.golang.org/api/container/v1"
	"google.golang.org/api/option"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gcp"
)

// apiClient is the [Client] backed by the GKE API client.
type apiClient struct {
	service *container.Service
	project string
}

var _ Client = (*apiClient)(nil)

func newAPIClient(ctx context.Context, project string, opts ...option.ClientOption) (*apiClient, error) {
	service, err := container.NewService(ctx, opts...)
	if err != nil {
		return nil, apiError(err)
	}
	return &apiClient{service: service, project: project}, nil
}

func (c *apiClient) Cluster(ctx context.Context, location, name string) (*Cluster, error) {
	cluster, err := c.service.Projects.Locations.Clusters.Get(c.clusterName(location, name)).Context(ctx).Do()
	if gcp.NotFound(err) {
		return nil, nil
	}
	if err != nil {
		return nil, apiError(err)
	}
	return cluster, nil
}

func (c *apiClient) SetNodePoolSize(ctx context.Context, location, cluster, pool string, count int64) (string, error) {
	name := c.clusterName(location, cluster) + "/nodePools/" + pool
	op, err := c.service.Projects.Locations.Clusters.NodePools.SetSize(name, &container.SetNodePoolSizeRequest{
		NodeCount:       count,
		ForceSendFields: []string{"NodeCount"},
	}).Context(ctx).Do()
	if err != nil {
		return "", apiError(err)
	}
	if op.Name == "" {
		return "", core.Providerf("GKE API returned an operation without a name")
	}
	return op.Name, nil
}

func (c *apiClient) clusterName(location, name string) string {
	return fmt.Sprintf("projects/%s/locations/%s/clusters/%s", c.project, location, name)
}

// apiError reports a failed API call as a provider error.
func apiError(err error) error {
	return core.Providerf("GKE API: %v", err)
}
