package volumedelete

import (
	"strings"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volume"
)

// snapshotLabel is the pre-deletion snapshot label that holds the numeric ID of its disk.
const snapshotLabel = "plural-sh-volume-delete"

// The Compute Engine snapshot statuses that end a snapshot.
const (
	snapshotStatusReady    = "READY"
	snapshotStatusFailed   = "FAILED"
	snapshotStatusDeleting = "DELETING"
)

// snapshots are the snapshots carrying [snapshotLabel] for a disk.
type snapshots []*compute.Snapshot

// status is the status of the latest snapshot of the disk diskID within the window.
func (s snapshots) status(diskID string, window volume.SnapshotWindow) volume.SnapshotStatus {
	latest := s.latest(diskID, window)
	if latest == nil {
		return volume.SnapshotStatus{State: volume.SnapshotMissing}
	}

	status := volume.SnapshotStatus{ID: latest.GetName()}
	switch latest.GetStatus() {
	case snapshotStatusReady:
		status.State = volume.SnapshotCompleted
	case snapshotStatusFailed, snapshotStatusDeleting:
		status.State = volume.SnapshotFailed
	default:
		status.State = volume.SnapshotInProgress
		if latest.Status != nil {
			status.Progress = new(strings.ToLower(latest.GetStatus()))
		}
	}
	return status
}

// latest is the most recently started snapshot of the disk diskID within the window, or nil
// if there is none. Of snapshots started at the same time, the last one listed wins.
func (s snapshots) latest(diskID string, window volume.SnapshotWindow) *compute.Snapshot {
	var latest *compute.Snapshot
	var latestCreated int64
	for _, snapshot := range s {
		// Compute Engine records the source disk itself, so a label copied onto another
		// snapshot doesn't match.
		if snapshot.GetSourceDiskId() != diskID {
			continue
		}
		created, ok := volume.ParseTimestamp(snapshot.GetCreationTimestamp())
		if ok && window.Counts(created) && (latest == nil || created >= latestCreated) {
			latest, latestCreated = snapshot, created
		}
	}
	return latest
}
