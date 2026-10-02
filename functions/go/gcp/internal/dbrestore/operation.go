package dbrestore

import (
	"context"
	"log/slog"
	"time"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/cloudsql"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// stateCreating is how the new instance is reported once its restore was submitted.
const stateCreating = "PENDING_CREATE"

// operation is a single invocation of the function, after its input was validated.
type operation struct {
	client cloudsql.Client
	clock  core.Clock
	action core.Action
	params Params
	point  time.Time
}

// run inspects the source and the target, evaluates the guards and submits the restore if
// they allow.
func (o *operation) run(ctx context.Context) (core.Response[Output], error) {
	found, err := o.client.Instance(ctx, o.params.Instance)
	if err != nil {
		return core.Response[Output]{}, err
	}
	if found == nil {
		return o.refuse(core.Guards{core.Fail(guardSource, "instance not found")}, Output{}), nil
	}
	src := source{found}
	var target *cloudsql.Instance
	if o.params.TargetInstance != o.params.Instance {
		if target, err = o.client.Instance(ctx, o.params.TargetInstance); err != nil {
			return core.Response[Output]{}, err
		}
	}

	guards := core.Guards{src.guard(), src.recoveryGuard(), targetGuard(o.params, target)}
	output := Output{Source: summary(found), Target: summary(target)}
	if src.recoverable() {
		reported, err := o.client.RecoveryWindow(ctx, o.params.Instance)
		if err != nil {
			return core.Response[Output]{}, err
		}
		w := newWindow(reported, src.logRetention(), time.Unix(o.clock.Now(), 0))
		guards = append(guards, w.guard(o.point))
		output.EarliestRestorePoint = reported.Earliest
		output.LatestRestorePoint = reported.Latest
	}
	if o.action == core.ActionPlan || !guards.Passed() {
		return o.refuse(guards, output), nil
	}

	op, err := o.client.Clone(ctx, o.params.Instance, o.params.TargetInstance, o.params.RestorePointInTime)
	if err != nil {
		return core.Response[Output]{}, err
	}
	slog.InfoContext(ctx, "restoring instance", "source", o.params.Instance, "target", o.params.TargetInstance,
		"point", o.params.RestorePointInTime, "operation", op)
	output.Target = &Summary{Name: o.params.TargetInstance, State: stateCreating}
	output.Submitted = true
	output.Operation = op
	return core.Done(guards, output), nil
}

// refuse answers a plan, or an execute that changes nothing.
func (o *operation) refuse(guards core.Guards, output Output) core.Response[Output] {
	if o.action == core.ActionPlan {
		return core.Planned(guards, output)
	}
	return core.Refused[Output](guards).WithResult(output)
}
