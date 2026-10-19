// Package iap is a thin Identity-Aware Proxy client, built on the Google API client, for the
// calls the functions make: who may open TCP tunnels to an instance.
//
// Functions use a [Client] obtained from a [Connector]. Credentials come from Application
// Default Credentials, which on Cloud Run is the service's own service account.
package iap

import (
	"context"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/iam"
)

// Client is the IAP API as the functions use it.
type Client interface {
	// TunnelPolicy returns the IAM policy of the IAP TCP tunnel to the instance.
	TunnelPolicy(ctx context.Context, zone, instance string) (*iam.Policy, error)
	// SetTunnelPolicy replaces the IAM policy of the IAP TCP tunnel to the instance, unless it
	// changed since it was read.
	SetTunnelPolicy(ctx context.Context, zone, instance string, policy *iam.Policy) error
}

// Connector provides the [Client] an invocation uses.
type Connector interface {
	Connect(ctx context.Context) (Client, error)
}
