package compute

import (
	"context"
	"errors"
	"fmt"
	"net/http"

	computev1 "cloud.google.com/go/compute/apiv1"
	"cloud.google.com/go/compute/apiv1/computepb"
	"google.golang.org/api/googleapi"
	"google.golang.org/api/iterator"
	"google.golang.org/api/option"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// sdkClient is the [Client] backed by the Compute Engine SDK.
type sdkClient struct {
	disks     *computev1.DisksClient
	snapshots *computev1.SnapshotsClient
	project   string
}

var _ Client = (*sdkClient)(nil)

func newSDKClient(ctx context.Context, project string, opts ...option.ClientOption) (*sdkClient, error) {
	disks, err := computev1.NewDisksRESTClient(ctx, opts...)
	if err != nil {
		return nil, sdkError(err)
	}
	snapshots, err := computev1.NewSnapshotsRESTClient(ctx, opts...)
	if err != nil {
		_ = disks.Close()
		return nil, sdkError(err)
	}
	return &sdkClient{disks: disks, snapshots: snapshots, project: project}, nil
}

func (c *sdkClient) Disk(ctx context.Context, zone, name string) (*Disk, error) {
	disk, err := c.disks.Get(ctx, &computepb.GetDiskRequest{Project: c.project, Zone: zone, Disk: name})
	if err != nil {
		if apiErr, ok := errors.AsType[*googleapi.Error](err); ok && apiErr.Code == http.StatusNotFound {
			return nil, nil
		}
		return nil, sdkError(err)
	}
	return disk, nil
}

func (c *sdkClient) Snapshots(ctx context.Context, label, value string) ([]*Snapshot, error) {
	it := c.snapshots.List(ctx, &computepb.ListSnapshotsRequest{
		Project: c.project,
		Filter:  new(fmt.Sprintf("labels.%s=%q", label, value)),
	})
	var snapshots []*Snapshot
	for {
		snapshot, err := it.Next()
		if errors.Is(err, iterator.Done) {
			return snapshots, nil
		}
		if err != nil {
			return nil, sdkError(err)
		}
		snapshots = append(snapshots, snapshot)
	}
}

func (c *sdkClient) CreateSnapshot(ctx context.Context, req SnapshotRequest) (string, error) {
	op, err := c.disks.CreateSnapshot(ctx, &computepb.CreateSnapshotDiskRequest{
		Project: c.project,
		Zone:    req.Zone,
		Disk:    req.Disk,
		SnapshotResource: &computepb.Snapshot{
			Name:        &req.Name,
			Description: &req.Description,
			Labels:      req.Labels,
		},
	})
	if err != nil {
		return "", sdkError(err)
	}
	return operationName(op)
}

func (c *sdkClient) DeleteDisk(ctx context.Context, zone, name string) (string, error) {
	op, err := c.disks.Delete(ctx, &computepb.DeleteDiskRequest{Project: c.project, Zone: zone, Disk: name})
	if err != nil {
		return "", sdkError(err)
	}
	return operationName(op)
}

// operationName is the name of an operation the API started.
func operationName(op *computev1.Operation) (string, error) {
	if op.Name() == "" {
		return "", core.Providerf("compute API returned an operation without a name")
	}
	return op.Name(), nil
}

// sdkError reports a failed API call as a provider error.
func sdkError(err error) error {
	return core.Providerf("compute API: %v", err)
}
