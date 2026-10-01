// Package volumedelete is the function that deletes an orphaned zonal persistent disk that
// Kubernetes created for a PersistentVolumeClaim.
//
// The guards and snapshot handling are shared with the other clouds, see [volume]. The
// pre-deletion snapshot is labelled with the disk's numeric ID, so it can't match a later
// disk with the same name.
package volumedelete

import (
	"context"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volume"
)

// Function handles the volume-delete invocations.
type Function struct {
	connector compute.Connector
	clock     core.Clock
}

var _ core.Handler[Params, Output] = (*Function)(nil)

// New returns the function acting on Compute Engine as the running service.
func New() *Function {
	return &Function{connector: compute.NewEnvConnector(), clock: core.SystemClock{}}
}

// Handle implements [core.Handler].
func (f *Function) Handle(ctx context.Context, req core.Request[Params]) (core.Response[Output], error) {
	var zero core.Response[Output]
	if err := req.Params.Validate(); err != nil {
		return zero, err
	}
	snapshotRequired, err := volume.SnapshotPolicyFromEnv().Required(req.Params.Snapshot)
	if err != nil {
		return zero, err
	}
	client, err := f.connector.Connect(ctx)
	if err != nil {
		return zero, err
	}

	op := operation{
		client:           client,
		clock:            f.clock,
		action:           req.Action,
		params:           req.Params,
		snapshotRequired: snapshotRequired,
	}
	return op.run(ctx)
}
