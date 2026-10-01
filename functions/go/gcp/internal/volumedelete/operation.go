package volumedelete

import (
	"context"
	"log/slog"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volume"
)

// operation is a single invocation of the function, after its input was validated.
type operation struct {
	client           compute.Client
	clock            core.Clock
	action           core.Action
	params           Params
	snapshotRequired bool
}

// target is what Compute Engine reports about the disk to delete.
type target struct {
	// volume is nil, and diskID empty, when the disk doesn't exist.
	volume   *volume.Volume
	diskID   string
	snapshot volume.SnapshotStatus
}

// run inspects the disk, evaluates the guards and takes the step they allow.
func (o *operation) run(ctx context.Context) (core.Response[Output], error) {
	target, err := o.inspect(ctx)
	if err != nil {
		return core.Response[Output]{}, err
	}

	guards, step := volume.Deletion{
		Action:           o.action,
		Volume:           target.volume,
		PV:               o.params.PVName,
		SnapshotRequired: o.snapshotRequired,
		Snapshot:         target.snapshot,
	}.Evaluate()
	output := Output{Volume: target.volume}
	if o.snapshotRequired {
		output.Snapshot = &target.snapshot
	}

	switch {
	case o.action == core.ActionPlan:
		return core.Planned(guards, output), nil
	case step == volume.StepCreateSnapshot:
		return o.createSnapshot(ctx, target.diskID, guards, output)
	case step == volume.StepDelete:
		return o.deleteDisk(ctx, guards, output)
	default:
		return core.Refused[Output](guards).WithResult(output), nil
	}
}

// inspect reads the disk and, if one is required, the state of its pre-deletion snapshot.
func (o *operation) inspect(ctx context.Context) (target, error) {
	found := target{snapshot: volume.SnapshotStatus{State: volume.SnapshotMissing}}
	resource, err := o.client.Disk(ctx, o.params.Zone, o.params.Disk)
	if err != nil || resource == nil {
		return found, err
	}

	d := disk{resource}
	found.volume = d.volume()
	if found.diskID, err = d.id(); err != nil {
		return found, err
	}
	if !o.snapshotRequired {
		return found, nil
	}
	taken, err := o.client.Snapshots(ctx, snapshotLabel, found.diskID)
	if err != nil {
		return found, err
	}
	window := volume.SnapshotWindow{Now: o.clock.Now(), DetachedAt: found.volume.DetachedAt}
	found.snapshot = snapshots(taken).status(found.diskID, window)
	return found, nil
}

// createSnapshot starts the pre-deletion snapshot and refuses the execute. A later execute
// deletes the disk once the snapshot completes.
func (o *operation) createSnapshot(ctx context.Context, diskID string, guards core.Guards, output Output) (core.Response[Output], error) {
	req := o.params.snapshotRequest(diskID, o.clock.Now())
	op, err := o.client.CreateSnapshot(ctx, req)
	if err != nil {
		return core.Response[Output]{}, err
	}
	slog.InfoContext(ctx, "started pre-deletion snapshot", "zone", req.Zone, "disk", req.Disk, "snapshot", req.Name)
	output.Snapshot = &volume.SnapshotStatus{State: volume.SnapshotInProgress, ID: req.Name}
	output.Operation = op
	return core.Refused[Output](guards).WithResult(output), nil
}

// deleteDisk starts deleting the disk.
func (o *operation) deleteDisk(ctx context.Context, guards core.Guards, output Output) (core.Response[Output], error) {
	op, err := o.client.DeleteDisk(ctx, o.params.Zone, o.params.Disk)
	if err != nil {
		return core.Response[Output]{}, err
	}
	slog.InfoContext(ctx, "deleting disk", "zone", o.params.Zone, "disk", o.params.Disk, "operation", op)
	output.Deleted = true
	output.Operation = op
	return core.Done(guards, output), nil
}
