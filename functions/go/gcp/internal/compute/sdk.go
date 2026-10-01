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
	instances *computev1.InstancesClient
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
	instances, err := computev1.NewInstancesRESTClient(ctx, opts...)
	if err != nil {
		_ = disks.Close()
		_ = snapshots.Close()
		return nil, sdkError(err)
	}
	return &sdkClient{disks: disks, snapshots: snapshots, instances: instances, project: project}, nil
}

func (c *sdkClient) Disk(ctx context.Context, zone, name string) (*Disk, error) {
	disk, err := c.disks.Get(ctx, &computepb.GetDiskRequest{Project: c.project, Zone: zone, Disk: name})
	return found(disk, err)
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

func (c *sdkClient) Instance(ctx context.Context, zone, name string) (*Instance, error) {
	instance, err := c.instances.Get(ctx, &computepb.GetInstanceRequest{Project: c.project, Zone: zone, Instance: name})
	return found(instance, err)
}

func (c *sdkClient) SetDiskAutoDelete(ctx context.Context, zone, instance, deviceName string, autoDelete bool) (string, error) {
	op, err := c.instances.SetDiskAutoDelete(ctx, &computepb.SetDiskAutoDeleteInstanceRequest{
		Project:    c.project,
		Zone:       zone,
		Instance:   instance,
		DeviceName: deviceName,
		AutoDelete: autoDelete,
	})
	if err != nil {
		return "", sdkError(err)
	}
	return operationName(op)
}

func (c *sdkClient) DeleteInstance(ctx context.Context, zone, name string) (string, error) {
	op, err := c.instances.Delete(ctx, &computepb.DeleteInstanceRequest{Project: c.project, Zone: zone, Instance: name})
	if err != nil {
		return "", sdkError(err)
	}
	return operationName(op)
}

// found is the resource a get returned, or nil if it doesn't exist.
func found[T any](resource *T, err error) (*T, error) {
	if err == nil {
		return resource, nil
	}
	if apiErr, ok := errors.AsType[*googleapi.Error](err); ok && apiErr.Code == http.StatusNotFound {
		return nil, nil
	}
	return nil, sdkError(err)
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
