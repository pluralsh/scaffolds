// Package compute is a thin Compute Engine client, built on the official SDK, for the calls
// the functions make.
//
// Functions use a [Client] obtained from a [Connector]. Credentials come from Application
// Default Credentials, which on Cloud Run is the service's own service account.
package compute

import (
	"context"

	"cloud.google.com/go/compute/apiv1/computepb"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/iam"
)

// The Compute Engine resources the functions read.
type (
	// Disk is a persistent disk.
	Disk = computepb.Disk
	// Snapshot is a snapshot of a persistent disk.
	Snapshot = computepb.Snapshot
	// Instance is a VM instance.
	Instance = computepb.Instance
	// AttachedDisk is a disk as attached to an instance.
	AttachedDisk = computepb.AttachedDisk
	// InstanceGroupManager is a managed instance group.
	InstanceGroupManager = computepb.InstanceGroupManager
)

// SnapshotRequest describes a snapshot to take of a disk.
type SnapshotRequest struct {
	Zone        string
	Disk        string
	Name        string
	Description string
	Labels      map[string]string
}

// Client is the Compute Engine API as the functions use it.
type Client interface {
	Disks
	Instances
	Groups
	Access
	LoadBalancers
}

// Disks are the disk and snapshot calls.
type Disks interface {
	// Disk returns the disk, or nil if it doesn't exist.
	Disk(ctx context.Context, zone, name string) (*Disk, error)
	// Snapshots returns all snapshots in the project with label=value, across all pages.
	Snapshots(ctx context.Context, label, value string) ([]*Snapshot, error)
	// CreateSnapshot starts a snapshot of a disk and returns the operation name.
	CreateSnapshot(ctx context.Context, req SnapshotRequest) (string, error)
	// DeleteDisk starts deleting the disk and returns the operation name.
	DeleteDisk(ctx context.Context, zone, name string) (string, error)
}

// Instances are the VM instance calls.
type Instances interface {
	// Instance returns the instance, or nil if it doesn't exist.
	Instance(ctx context.Context, zone, name string) (*Instance, error)
	// SetDiskAutoDelete starts setting whether the disk attached to the instance as deviceName
	// is deleted with it, and returns the operation name.
	SetDiskAutoDelete(ctx context.Context, zone, instance, deviceName string, autoDelete bool) (string, error)
	// DeleteInstance starts deleting the instance and returns the operation name.
	DeleteInstance(ctx context.Context, zone, name string) (string, error)
}

// Groups are the managed instance group calls.
type Groups interface {
	// InstanceGroupManager returns the managed instance group, or nil if it doesn't exist.
	InstanceGroupManager(ctx context.Context, zone, name string) (*InstanceGroupManager, error)
}

// Access are the calls that manage who may log in to instances.
type Access interface {
	// ProjectMetadata returns the project's metadata, which instances inherit.
	ProjectMetadata(ctx context.Context) (map[string]string, error)
	// InstancePolicy returns the IAM policy of the instance.
	InstancePolicy(ctx context.Context, zone, name string) (*iam.Policy, error)
	// SetInstancePolicy replaces the IAM policy of the instance, unless it changed since it was
	// read.
	SetInstancePolicy(ctx context.Context, zone, name string, policy *iam.Policy) error
}

// LoadBalancers are the calls on the resources of network load balancers.
type LoadBalancers interface {
	// LBResource returns the resource, or nil if it doesn't exist. region is ignored for global
	// kinds.
	LBResource(ctx context.Context, kind LBKind, region, name string) (*LBResource, error)
	// ForwardingRules returns the forwarding rules of the region, across all pages.
	ForwardingRules(ctx context.Context, region string) ([]*LBResource, error)
	// Healthy reports whether the target pool or backend service reports the member, an
	// instance or instance group URL, as healthy. A member that no longer exists isn't.
	Healthy(ctx context.Context, target *LBResource, member string) (bool, error)
	// DeleteLBResource starts deleting the resource and returns the operation name. If Compute
	// Engine refuses because the resource is busy or in use, the error wraps [ErrBusy].
	DeleteLBResource(ctx context.Context, kind LBKind, region, name string) (string, error)
}

// Connector provides the [Client] an invocation uses.
type Connector interface {
	Connect(ctx context.Context) (Client, error)
}
