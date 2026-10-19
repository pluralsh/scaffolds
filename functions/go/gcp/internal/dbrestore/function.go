// Package dbrestore is the function that restores a Cloud SQL instance to a point in time, as
// a new instance.
//
// Restores always create a new instance, a point-in-time clone of the source; the source is
// never changed. The clone gets the source's settings, such as its tier, network, database
// flags and labels. A restore outlasts one invocation: execute submits it, and plan with the
// same parameters then reports the new instance's state and addresses.
//
// Cloud SQL can't label a clone when creating it, and labelling it afterwards would need
// permission to change every instance, so an existing instance under the target name isn't
// recognised as an earlier restore: it is reported and refused, never changed.
package dbrestore

import (
	"context"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/cloudsql"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// Function handles the db-restore invocations.
type Function struct {
	connector cloudsql.Connector
	clock     core.Clock
}

var _ core.Handler[Params, Output] = (*Function)(nil)

// New returns the function acting on Cloud SQL as the running service.
func New() *Function {
	return &Function{connector: cloudsql.NewEnvConnector(), clock: core.SystemClock{}}
}

// Handle implements [core.Handler].
func (f *Function) Handle(ctx context.Context, req core.Request[Params]) (core.Response[Output], error) {
	var zero core.Response[Output]
	point, err := req.Params.Validate()
	if err != nil {
		return zero, err
	}
	client, err := f.connector.Connect(ctx)
	if err != nil {
		return zero, err
	}

	op := operation{client: client, clock: f.clock, action: req.Action, params: req.Params, point: point}
	return op.run(ctx)
}
