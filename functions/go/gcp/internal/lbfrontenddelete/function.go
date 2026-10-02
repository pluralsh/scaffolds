// Package lbfrontenddelete is the function that removes what a deleted Kubernetes LoadBalancer
// Service left of its GKE load balancer.
//
// For a LoadBalancer Service, the GKE cloud provider creates a regional forwarding rule named
// after the Service UID (`a` and its first 31 hex digits), a target pool (external) or
// backend service (internal) of the same name, a health check of that name for Services with
// externalTrafficPolicy Local, firewall rules `k8s-fw-<name>` and `k8s-<name>-http-hc`, and
// a static address of that name. Each carries the Service's namespace/name in its
// description. The function only deletes resources of that name whose description names the
// Service; the cluster-wide node health check and its firewall rule are shared and never
// touched.
//
// Deleting takes several executes, as each resource can only go once nothing uses it: the
// forwarding rule first, then the target, firewall rules and address, then the health check.
// The result reports remaining: true while a later execute has more to delete.
//
// Load balancers created by GKE's own L4 controller (names starting with k8s2-, used with GKE
// subsetting and backend service based external load balancers) clean up after themselves and
// aren't supported.
package lbfrontenddelete

import (
	"context"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// Function handles the lb-frontend-delete invocations.
type Function struct {
	connector compute.Connector
}

var _ core.Handler[Params, Output] = (*Function)(nil)

// New returns the function acting on Compute Engine as the running service.
func New() *Function {
	return &Function{connector: compute.NewEnvConnector()}
}

// Handle implements [core.Handler].
func (f *Function) Handle(ctx context.Context, req core.Request[Params]) (core.Response[Output], error) {
	var zero core.Response[Output]
	if err := req.Params.Validate(); err != nil {
		return zero, err
	}
	client, err := f.connector.Connect(ctx)
	if err != nil {
		return zero, err
	}

	op := operation{client: client, action: req.Action, params: req.Params}
	return op.run(ctx)
}
