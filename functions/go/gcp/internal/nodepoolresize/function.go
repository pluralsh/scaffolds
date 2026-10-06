// Package nodepoolresize is the function that sets the node count of a manually scaled GKE
// node pool.
//
// GKE counts nodes per zone: the pool gets count nodes in each of its zones. GKE drains the
// nodes it removes. Pools scaled by the cluster autoscaler are refused; change their minimum
// and maximum instead. So are Autopilot clusters, whose nodes GKE manages. The total node
// count is capped by the installation's [MaxCountVar], and the cluster's only node pool keeps
// at least one node per zone.
package nodepoolresize

import (
	"context"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gke"
)

// Function handles the node-pool-resize invocations.
type Function struct {
	gke      gke.Connector
	compute  compute.Connector
	maxCount int64
}

var _ core.Handler[Params, Output] = (*Function)(nil)

// New returns the function acting on GKE and Compute Engine as the running service, with the
// cap from the environment.
func New() *Function {
	return &Function{gke: gke.NewEnvConnector(), compute: compute.NewEnvConnector(), maxCount: MaxCountFromEnv()}
}

// Handle implements [core.Handler].
func (f *Function) Handle(ctx context.Context, req core.Request[Params]) (core.Response[Output], error) {
	var zero core.Response[Output]
	if err := req.Params.Validate(); err != nil {
		return zero, err
	}
	clusters, err := f.gke.Connect(ctx)
	if err != nil {
		return zero, err
	}
	groups, err := f.compute.Connect(ctx)
	if err != nil {
		return zero, err
	}

	op := operation{clusters: clusters, groups: groups, action: req.Action, params: req.Params, maxCount: f.maxCount}
	return op.run(ctx)
}
