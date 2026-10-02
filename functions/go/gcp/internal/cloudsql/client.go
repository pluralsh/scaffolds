// Package cloudsql is a thin Cloud SQL Admin client, built on the Google API client, for the
// calls the functions make.
//
// Functions use a [Client] obtained from a [Connector]. Credentials come from Application
// Default Credentials, which on Cloud Run is the service's own service account.
package cloudsql

import (
	"context"

	sqladmin "google.golang.org/api/sqladmin/v1"
)

// Instance is a Cloud SQL instance.
type Instance = sqladmin.DatabaseInstance

// RecoveryWindow is the time range an instance can be restored to, as RFC 3339 timestamps.
// Either end is empty if Cloud SQL doesn't report it.
type RecoveryWindow struct {
	Earliest string
	Latest   string
}

// Client is the Cloud SQL Admin API as the functions use it.
type Client interface {
	// Instance returns the instance, or nil if it doesn't exist.
	Instance(ctx context.Context, name string) (*Instance, error)
	// RecoveryWindow returns the time range the instance can be restored to.
	RecoveryWindow(ctx context.Context, name string) (RecoveryWindow, error)
	// Clone starts creating the instance target as a copy of source at the point in time (RFC
	// 3339) and returns the operation name.
	Clone(ctx context.Context, source, target, pointInTime string) (string, error)
}

// Connector provides the [Client] an invocation uses.
type Connector interface {
	Connect(ctx context.Context) (Client, error)
}
