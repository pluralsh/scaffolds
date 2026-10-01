// Package vmdelete is the function that deletes a standalone Compute Engine instance together
// with its boot disk.
//
// Data disks are detached and kept; delete them with volume-delete once they are no longer
// needed. Instances created by a managed instance group, including GKE nodes, are refused:
// resize the group or node pool instead so it doesn't recreate them. Local SSDs are always
// deleted with the instance, so the plan lists them.
//
// Compute Engine deletes the attached disks whose auto-delete flag is set. execute first sets
// it on the boot disk and clears it on the data disks, then deletes the instance. Compute
// Engine applies the flags asynchronously, so execute is usually refused after the first step
// and a later execute deletes the instance.
package vmdelete

import (
	"context"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// Function handles the vm-delete invocations.
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
