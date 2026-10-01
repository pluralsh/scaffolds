// Package compute is a thin Compute Engine client, built on the official SDK, for the calls
// the functions make.
//
// Functions use a [Client] obtained from a [Connector]. Credentials come from Application
// Default Credentials, which on Cloud Run is the service's own service account.
package compute

import (
	"context"

	"cloud.google.com/go/compute/apiv1/computepb"
)

// The Compute Engine resources the functions read.
type (
	// Disk is a persistent disk.
	Disk = computepb.Disk
	// Snapshot is a snapshot of a persistent disk.
	Snapshot = computepb.Snapshot
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
	// Disk returns the disk, or nil if it doesn't exist.
	Disk(ctx context.Context, zone, name string) (*Disk, error)
	// Snapshots returns all snapshots in the project with label=value, across all pages.
	Snapshots(ctx context.Context, label, value string) ([]*Snapshot, error)
	// CreateSnapshot starts a snapshot of a disk and returns the operation name.
	CreateSnapshot(ctx context.Context, req SnapshotRequest) (string, error)
	// DeleteDisk starts deleting the disk and returns the operation name.
	DeleteDisk(ctx context.Context, zone, name string) (string, error)
}

// Connector provides the [Client] an invocation uses.
type Connector interface {
	Connect(ctx context.Context) (Client, error)
}
