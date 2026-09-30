package volumedelete

import (
	"testing"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volume"
)

func TestPicksLatestRecentSnapshot(t *testing.T) {
	now := mustTimestamp(t, "2026-09-30T12:00:00Z")
	tests := []struct {
		name      string
		snapshots []*compute.Snapshot
		want      volume.SnapshotStatus
	}{
		{"none", nil, volume.SnapshotStatus{State: volume.SnapshotMissing}},
		{
			"latest wins",
			[]*compute.Snapshot{
				testSnapshot("old", "2026-09-30T03:00:00.000-07:00", "READY"),
				testSnapshot("new", "2026-09-30T11:00:00Z", "UPLOADING"),
			},
			volume.SnapshotStatus{State: volume.SnapshotInProgress, ID: "new", Progress: new("uploading")},
		},
		{
			"latest wins in any order",
			[]*compute.Snapshot{
				testSnapshot("new", "2026-09-30T11:00:00Z", "UPLOADING"),
				testSnapshot("old", "2026-09-30T03:00:00.000-07:00", "READY"),
			},
			volume.SnapshotStatus{State: volume.SnapshotInProgress, ID: "new", Progress: new("uploading")},
		},
		{
			"ties go to the last one",
			[]*compute.Snapshot{
				testSnapshot("a", "2026-09-30T11:00:00Z", "READY"),
				testSnapshot("b", "2026-09-30T11:00:00Z", "READY"),
			},
			volume.SnapshotStatus{State: volume.SnapshotCompleted, ID: "b"},
		},
		{
			"completed",
			[]*compute.Snapshot{testSnapshot("s", "2026-09-30T11:00:00Z", "READY")},
			volume.SnapshotStatus{State: volume.SnapshotCompleted, ID: "s"},
		},
		{
			"failed",
			[]*compute.Snapshot{testSnapshot("s", "2026-09-30T11:00:00Z", "FAILED")},
			volume.SnapshotStatus{State: volume.SnapshotFailed, ID: "s"},
		},
		{
			"deleting counts as failed",
			[]*compute.Snapshot{testSnapshot("s", "2026-09-30T11:00:00Z", "DELETING")},
			volume.SnapshotStatus{State: volume.SnapshotFailed, ID: "s"},
		},
		{
			"creating is in progress",
			[]*compute.Snapshot{testSnapshot("s", "2026-09-30T11:00:00Z", "CREATING")},
			volume.SnapshotStatus{State: volume.SnapshotInProgress, ID: "s", Progress: new("creating")},
		},
		{
			// A snapshot left by an earlier attempt doesn't stand in for data written since.
			"too old",
			[]*compute.Snapshot{testSnapshot("s", "2026-09-28T11:00:00Z", "READY")},
			volume.SnapshotStatus{State: volume.SnapshotMissing},
		},
		{
			"unparsable timestamp",
			[]*compute.Snapshot{testSnapshot("s", "yesterday", "READY")},
			volume.SnapshotStatus{State: volume.SnapshotMissing},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := snapshots(tt.snapshots).status("4242", volume.SnapshotWindow{Now: now})

			if !equalStatus(got, tt.want) {
				t.Errorf("status() = %+v, want %+v", got, tt.want)
			}
		})
	}
}

func TestOnlyCountsSnapshotsOfThisDiskAfterItsLastDetach(t *testing.T) {
	now := mustTimestamp(t, "2026-09-30T12:00:00Z")
	otherDisk := testSnapshot("copy", "2026-09-30T11:00:00Z", "READY")
	otherDisk.SourceDiskId = new("9999")
	beforeDetach := testSnapshot("s", "2026-09-30T10:00:00Z", "READY")
	detachedAt := new(mustTimestamp(t, "2026-09-30T10:30:00Z"))

	// Labelled for this disk, but Compute Engine recorded another source disk.
	if got := (snapshots{otherDisk}).status("4242", volume.SnapshotWindow{Now: now}); got.State != volume.SnapshotMissing {
		t.Errorf("other disk: %+v", got)
	}
	// Written to again after the snapshot: the snapshot misses that data.
	if got := (snapshots{beforeDetach}).status("4242", volume.SnapshotWindow{Now: now, DetachedAt: detachedAt}); got.State != volume.SnapshotMissing {
		t.Errorf("before detach: %+v", got)
	}
	want := volume.SnapshotStatus{State: volume.SnapshotCompleted, ID: "s"}
	if got := (snapshots{beforeDetach}).status("4242", volume.SnapshotWindow{Now: now}); !equalStatus(got, want) {
		t.Errorf("never detached: %+v", got)
	}
}

func equalStatus(a, b volume.SnapshotStatus) bool {
	sameProgress := a.Progress == b.Progress || (a.Progress != nil && b.Progress != nil && *a.Progress == *b.Progress)
	return a.State == b.State && a.ID == b.ID && sameProgress
}
