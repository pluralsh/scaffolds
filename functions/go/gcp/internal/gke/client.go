// Package gke is a thin Google Kubernetes Engine client, built on the Google API client, for
// the calls the functions make.
//
// Functions use a [Client] obtained from a [Connector]. Credentials come from Application
// Default Credentials, which on Cloud Run is the service's own service account.
package gke

import (
	"context"

	container "google.golang.org/api/container/v1"
)

// The GKE resources the functions read.
type (
	// Cluster is a GKE cluster, with its node pools.
	Cluster = container.Cluster
	// NodePool is a node pool of a cluster.
	NodePool = container.NodePool
)

// Client is the GKE API as the functions use it.
type Client interface {
	// Cluster returns the cluster in the location (a region or a zone), or nil if it doesn't
	// exist.
	Cluster(ctx context.Context, location, name string) (*Cluster, error)
	// SetNodePoolSize starts setting the node count of each zone of the node pool and returns
	// the operation name.
	SetNodePoolSize(ctx context.Context, location, cluster, pool string, count int64) (string, error)
}

// Connector provides the [Client] an invocation uses.
type Connector interface {
	Connect(ctx context.Context) (Client, error)
}
