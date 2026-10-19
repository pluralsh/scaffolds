package compute

import (
	"context"
	"errors"
	"fmt"
	"io"
	"slices"
	"strings"

	computev1 "cloud.google.com/go/compute/apiv1"
	"cloud.google.com/go/compute/apiv1/computepb"
	computeapi "google.golang.org/api/compute/v1"
	"google.golang.org/api/iterator"
	"google.golang.org/api/option"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gcp"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/iam"
)

// sdkClient is the [Client] backed by the Compute Engine SDK.
type sdkClient struct {
	disks     *computev1.DisksClient
	snapshots *computev1.SnapshotsClient
	instances *computev1.InstancesClient
	groups    *computev1.InstanceGroupManagersClient
	projects  *computev1.ProjectsClient
	lb        lbClients
	project   string
}

var _ Client = (*sdkClient)(nil)

// newSDKClient creates the client. endpoint is the API host to use in place of Compute
// Engine's, or empty.
func newSDKClient(ctx context.Context, project, endpoint string, opts ...option.ClientOption) (*sdkClient, error) {
	c := &sdkClient{project: project}
	sdkOpts, legacyOpts := opts, opts
	if endpoint != "" {
		sdkOpts = append(slices.Clone(opts), option.WithEndpoint(endpoint))
		legacyOpts = append(slices.Clone(opts), option.WithEndpoint(strings.TrimSuffix(endpoint, "/")+legacyBasePath))
	}
	o := opener{ctx: ctx, opts: sdkOpts}
	c.disks = open(&o, computev1.NewDisksRESTClient)
	c.snapshots = open(&o, computev1.NewSnapshotsRESTClient)
	c.instances = open(&o, computev1.NewInstancesRESTClient)
	c.groups = open(&o, computev1.NewInstanceGroupManagersRESTClient)
	c.projects = open(&o, computev1.NewProjectsRESTClient)
	c.lb = lbClients{
		forwardingRules: open(&o, computev1.NewForwardingRulesRESTClient),
		targetPools:     open(&o, computev1.NewTargetPoolsRESTClient),
		backendServices: open(&o, computev1.NewRegionBackendServicesRESTClient),
		healthChecks:    open(&o, computev1.NewHealthChecksRESTClient),
		firewalls:       open(&o, computev1.NewFirewallsRESTClient),
		addresses:       open(&o, computev1.NewAddressesRESTClient),
	}
	if o.err == nil {
		var legacy *computeapi.Service
		if legacy, o.err = computeapi.NewService(ctx, legacyOpts...); o.err == nil {
			c.lb.httpHealthChecks = legacy.HttpHealthChecks
		}
	}
	if o.err != nil {
		for _, client := range o.opened {
			_ = client.Close()
		}
		return nil, sdkError(o.err)
	}
	return c, nil
}

// legacyBasePath is the path of the Compute Engine API in the Google API client, which serves
// the legacy HTTP health checks the SDK no longer has.
const legacyBasePath = "/compute/v1/"

// opener creates the SDK clients, until one fails.
type opener struct {
	ctx    context.Context
	opts   []option.ClientOption
	opened []io.Closer
	err    error
}

// open creates a client with create, unless creating an earlier one failed.
func open[T io.Closer](o *opener, create func(context.Context, ...option.ClientOption) (T, error)) T {
	var zero T
	if o.err != nil {
		return zero
	}
	client, err := create(o.ctx, o.opts...)
	if err != nil {
		o.err = err
		return zero
	}
	o.opened = append(o.opened, client)
	return client
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

func (c *sdkClient) InstanceGroupManager(ctx context.Context, zone, name string) (*InstanceGroupManager, error) {
	group, err := c.groups.Get(ctx, &computepb.GetInstanceGroupManagerRequest{Project: c.project, Zone: zone, InstanceGroupManager: name})
	return found(group, err)
}

func (c *sdkClient) ProjectMetadata(ctx context.Context) (map[string]string, error) {
	project, err := c.projects.Get(ctx, &computepb.GetProjectRequest{Project: c.project})
	if err != nil {
		return nil, sdkError(err)
	}
	metadata := make(map[string]string)
	for _, item := range project.GetCommonInstanceMetadata().GetItems() {
		metadata[item.GetKey()] = item.GetValue()
	}
	return metadata, nil
}

func (c *sdkClient) InstancePolicy(ctx context.Context, zone, name string) (*iam.Policy, error) {
	policy, err := c.instances.GetIamPolicy(ctx, &computepb.GetIamPolicyInstanceRequest{
		Project:                       c.project,
		Zone:                          zone,
		Resource:                      name,
		OptionsRequestedPolicyVersion: new(int32(iam.Version)),
	})
	if err != nil {
		return nil, sdkError(err)
	}
	return fromComputePolicy(policy), nil
}

func (c *sdkClient) SetInstancePolicy(ctx context.Context, zone, name string, policy *iam.Policy) error {
	_, err := c.instances.SetIamPolicy(ctx, &computepb.SetIamPolicyInstanceRequest{
		Project:                      c.project,
		Zone:                         zone,
		Resource:                     name,
		ZoneSetPolicyRequestResource: &computepb.ZoneSetPolicyRequest{Policy: toComputePolicy(policy)},
	})
	if err != nil {
		return sdkError(err)
	}
	return nil
}

func fromComputePolicy(policy *computepb.Policy) *iam.Policy {
	p := &iam.Policy{Etag: policy.GetEtag()}
	for _, b := range policy.GetBindings() {
		binding := &iam.Binding{Role: b.GetRole(), Members: b.GetMembers()}
		if cond := b.GetCondition(); cond != nil {
			binding.Condition = &iam.Condition{Title: cond.GetTitle(), Description: cond.GetDescription(), Expression: cond.GetExpression()}
		}
		p.Bindings = append(p.Bindings, binding)
	}
	return p
}

func toComputePolicy(policy *iam.Policy) *computepb.Policy {
	p := &computepb.Policy{Etag: &policy.Etag, Version: new(int32(iam.Version))}
	for _, b := range policy.Bindings {
		binding := &computepb.Binding{Role: &b.Role, Members: b.Members}
		if cond := b.Condition; cond != nil {
			binding.Condition = &computepb.Expr{Title: &cond.Title, Description: &cond.Description, Expression: &cond.Expression}
		}
		p.Bindings = append(p.Bindings, binding)
	}
	return p
}

// found is the resource a get returned, or nil if it doesn't exist.
func found[T any](resource *T, err error) (*T, error) {
	if err == nil {
		return resource, nil
	}
	if gcp.NotFound(err) {
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
