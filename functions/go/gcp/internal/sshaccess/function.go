// Package sshaccess is the function that grants a user short-lived SSH access to a Compute
// Engine instance, through OS Login and an Identity-Aware Proxy TCP tunnel.
//
// No keys are pushed to the instance and no port is opened to the internet: the user connects
// with `gcloud compute ssh --tunnel-through-iap`, which needs an OS Login role on the instance
// and permission to tunnel to it through IAP. The function grants both as instance-level IAM
// bindings whose condition expires them, so the access ends without any cleanup. Expired
// bindings are still removed by every execute on the instance, to keep the policies small.
//
// Granting again extends the access and never shortens it; revoke removes it right away.
package sshaccess

import (
	"context"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gcp"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/iap"
)

// Function handles the ssh-access invocations.
type Function struct {
	compute    compute.Connector
	iap        iap.Connector
	clock      core.Clock
	maxMinutes int64
}

var _ core.Handler[Params, Output] = (*Function)(nil)

// New returns the function acting on Compute Engine and IAP as the running service, with the
// limit from the environment.
func New() *Function {
	return &Function{compute: compute.NewEnvConnector(), iap: iap.NewEnvConnector(), clock: core.SystemClock{}, maxMinutes: MaxMinutesFromEnv()}
}

// Handle implements [core.Handler].
func (f *Function) Handle(ctx context.Context, req core.Request[Params]) (core.Response[Output], error) {
	var zero core.Response[Output]
	if err := req.Params.Validate(); err != nil {
		return zero, err
	}
	project, err := gcp.Project()
	if err != nil {
		return zero, err
	}
	instances, err := f.compute.Connect(ctx)
	if err != nil {
		return zero, err
	}
	tunnels, err := f.iap.Connect(ctx)
	if err != nil {
		return zero, err
	}

	op := operation{
		instances:  instances,
		tunnels:    tunnels,
		project:    project,
		now:        f.clock.Now(),
		action:     req.Action,
		params:     req.Params.withDefaults(),
		maxMinutes: f.maxMinutes,
	}
	return op.run(ctx)
}
