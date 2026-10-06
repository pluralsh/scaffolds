package cloudsql

import (
	"context"

	"google.golang.org/api/option"
	sqladmin "google.golang.org/api/sqladmin/v1"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gcp"
)

// apiClient is the [Client] backed by the Cloud SQL Admin API client.
type apiClient struct {
	service *sqladmin.Service
	project string
}

var _ Client = (*apiClient)(nil)

func newAPIClient(ctx context.Context, project string, opts ...option.ClientOption) (*apiClient, error) {
	service, err := sqladmin.NewService(ctx, opts...)
	if err != nil {
		return nil, apiError(err)
	}
	return &apiClient{service: service, project: project}, nil
}

func (c *apiClient) Instance(ctx context.Context, name string) (*Instance, error) {
	instance, err := c.service.Instances.Get(c.project, name).Context(ctx).Do()
	if gcp.NotFound(err) {
		return nil, nil
	}
	if err != nil {
		return nil, apiError(err)
	}
	return instance, nil
}

func (c *apiClient) RecoveryWindow(ctx context.Context, name string) (RecoveryWindow, error) {
	resp, err := c.service.Projects.Instances.GetLatestRecoveryTime(c.project, name).Context(ctx).Do()
	if err != nil {
		return RecoveryWindow{}, apiError(err)
	}
	return RecoveryWindow{Earliest: resp.EarliestRecoveryTime, Latest: resp.LatestRecoveryTime}, nil
}

func (c *apiClient) Clone(ctx context.Context, source, target, pointInTime string) (string, error) {
	op, err := c.service.Instances.Clone(c.project, source, &sqladmin.InstancesCloneRequest{
		CloneContext: &sqladmin.CloneContext{DestinationInstanceName: target, PointInTime: pointInTime},
	}).Context(ctx).Do()
	if err != nil {
		return "", apiError(err)
	}
	if op.Name == "" {
		return "", core.Providerf("Cloud SQL Admin API returned an operation without a name")
	}
	return op.Name, nil
}

// apiError reports a failed API call as a provider error.
func apiError(err error) error {
	return core.Providerf("Cloud SQL Admin API: %v", err)
}
