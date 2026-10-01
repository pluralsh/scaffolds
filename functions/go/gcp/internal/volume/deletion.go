package volume

import (
	"fmt"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// The names of the guards, in the order they are evaluated.
const (
	guardExists     = "exists"
	guardUnattached = "unattached"
	guardState      = "state"
	guardKubernetes = "kubernetes"
	guardSnapshot   = "snapshot"
)

// Step is what the function should do after evaluating the guards.
type Step int

const (
	// StepNothing changes nothing: a plan, or a guard failed.
	StepNothing Step = iota
	// StepCreateSnapshot starts a snapshot and refuses. A later execute deletes the volume.
	StepCreateSnapshot
	// StepDelete deletes the volume.
	StepDelete
)

// Deletion is a request to delete a volume, with what the cloud reports about it.
type Deletion struct {
	Action core.Action
	// Volume is nil when the volume doesn't exist.
	Volume *Volume
	// PV is the PersistentVolume the caller says the volume was created for.
	PV PVName
	// SnapshotRequired is whether a completed snapshot is needed before deleting.
	SnapshotRequired bool
	// Snapshot is ignored unless SnapshotRequired is set.
	Snapshot SnapshotStatus
}

// Evaluate evaluates the guards for the deletion and decides the next step.
func (d Deletion) Evaluate() (core.Guards, Step) {
	if d.Volume == nil {
		return core.Guards{core.Fail(guardExists, "volume not found")}, StepNothing
	}

	guards := d.Volume.guards(d.PV)
	safe := guards.Passed()
	guard, step := d.snapshotGuard()
	guards = append(guards, guard)

	if d.Action == core.ActionPlan || !safe {
		step = StepNothing
	}
	return guards, step
}

// snapshotGuard checks the pre-deletion snapshot. The step it returns applies only if every
// other guard passed.
func (d Deletion) snapshotGuard() (core.Guard, Step) {
	snapshot := d.Snapshot
	switch {
	case !d.SnapshotRequired:
		return core.Pass(guardSnapshot, "not requested"), StepDelete
	case snapshot.State == SnapshotCompleted:
		return core.Pass(guardSnapshot, fmt.Sprintf("snapshot %s completed", snapshot.ID)), StepDelete
	case snapshot.State == SnapshotInProgress:
		progress := ""
		if snapshot.Progress != nil {
			progress = fmt.Sprintf(" (%s)", *snapshot.Progress)
		}
		detail := fmt.Sprintf("snapshot %s is in progress%s; execute again once it completes", snapshot.ID, progress)
		return core.Fail(guardSnapshot, detail), StepNothing
	case d.Action != core.ActionPlan:
		return core.Fail(guardSnapshot, "no completed snapshot yet"), StepCreateSnapshot
	case snapshot.State == SnapshotFailed:
		detail := fmt.Sprintf("snapshot %s failed; execute takes a new snapshot first and deletes the volume in a later execute", snapshot.ID)
		return core.Pass(guardSnapshot, detail), StepCreateSnapshot
	default:
		return core.Pass(guardSnapshot, "execute takes a snapshot first and deletes the volume in a later execute"), StepCreateSnapshot
	}
}
