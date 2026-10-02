package vmdelete

import (
	"context"
	"log/slog"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// operation is a single invocation of the function, after its input was validated.
type operation struct {
	client compute.Instances
	action core.Action
	params Params
}

// run inspects the instance, evaluates the guards and takes the step they allow.
func (o *operation) run(ctx context.Context) (core.Response[Output], error) {
	found, err := o.client.Instance(ctx, o.params.Zone, o.params.Instance)
	if err != nil {
		return core.Response[Output]{}, err
	}
	if found == nil {
		guards := core.Guards{core.Fail(guardExists, "instance not found")}
		return o.refuse(guards, Output{}), nil
	}

	target := instance{found}
	guards := target.guards()
	output := Output{Instance: target.summary(), AutoDeleteSet: target.autoDeleteSet()}
	if o.action == core.ActionPlan || !guards.Passed() {
		return o.refuse(guards, output), nil
	}

	if !target.autoDeleteSet() {
		ready, err := o.setAutoDelete(ctx, target, &output)
		if err != nil {
			return core.Response[Output]{}, err
		}
		if !ready {
			guards = append(guards, core.Fail(guardAutoDelete,
				"the instance is updating the auto-delete flags of its disks; execute again once it has finished"))
			return core.Refused[Output](guards).WithResult(output), nil
		}
	}
	return o.deleteInstance(ctx, guards, output)
}

// refuse answers a plan, or an execute that changes nothing.
func (o *operation) refuse(guards core.Guards, output Output) core.Response[Output] {
	if o.action == core.ActionPlan {
		return core.Planned(guards, output)
	}
	return core.Refused[Output](guards).WithResult(output)
}

// setAutoDelete starts setting the auto-delete flag on the boot disk and clearing it on the
// data disks, then reports whether the instance already shows them set and can be deleted.
func (o *operation) setAutoDelete(ctx context.Context, target instance, output *Output) (bool, error) {
	for _, disk := range target.persistentDisks() {
		if disk.GetAutoDelete() == disk.GetBoot() {
			continue
		}
		if _, err := o.client.SetDiskAutoDelete(ctx, o.params.Zone, o.params.Instance, disk.GetDeviceName(), disk.GetBoot()); err != nil {
			return false, err
		}
	}
	slog.InfoContext(ctx, "set disk auto-delete flags", "zone", o.params.Zone, "instance", o.params.Instance)
	output.AutoDeleteSet = true

	updated, err := o.client.Instance(ctx, o.params.Zone, o.params.Instance)
	if err != nil || updated == nil {
		return false, err
	}
	current := instance{updated}
	output.Instance = current.summary()
	return current.autoDeleteSet() && current.idle(), nil
}

// deleteInstance starts deleting the instance.
func (o *operation) deleteInstance(ctx context.Context, guards core.Guards, output Output) (core.Response[Output], error) {
	op, err := o.client.DeleteInstance(ctx, o.params.Zone, o.params.Instance)
	if err != nil {
		return core.Response[Output]{}, err
	}
	slog.InfoContext(ctx, "deleting instance", "zone", o.params.Zone, "instance", o.params.Instance, "operation", op)
	output.Deleted = true
	output.Operation = op
	return core.Done(guards, output), nil
}
