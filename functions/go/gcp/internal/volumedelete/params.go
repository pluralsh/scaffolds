package volumedelete

import (
	"fmt"
	"strings"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volume"
)

// snapshotNameDiskLen is how much of the disk name a snapshot name keeps, leaving room for
// its suffix within the 63 characters of a resource name.
const snapshotNameDiskLen = 40

// Params are the operation-specific fields of the tool input.
type Params struct {
	Zone string `json:"zone" required:"true"`
	Disk string `json:"disk" required:"true"`
	// PVName is the PersistentVolume the disk was created for; the caller confirms it no
	// longer exists.
	PVName volume.PVName `json:"pvName" required:"true"`
	// Snapshot takes a snapshot and waits for it to complete before deleting. Defaults to true.
	Snapshot *bool `json:"snapshot"`
}

// Validate checks the parameters before anything is asked of Compute Engine.
func (p Params) Validate() error {
	if !compute.Zone(p.Zone).Valid() {
		return core.InvalidRequestf("zone %q is not a Compute Engine zone (e.g. us-central1-a)", p.Zone)
	}
	if !compute.ResourceName(p.Disk).Valid() {
		return core.InvalidRequestf("disk %q is not a Compute Engine disk name", p.Disk)
	}
	return p.PVName.Validate()
}

// snapshotRequest is the pre-deletion snapshot of the disk with the numeric ID diskID,
// started at now (Unix seconds).
func (p Params) snapshotRequest(diskID string, now int64) compute.SnapshotRequest {
	return compute.SnapshotRequest{
		Zone:        p.Zone,
		Disk:        p.Disk,
		Name:        p.snapshotName(now),
		Description: fmt.Sprintf("Taken by Plural before deleting disk %s in %s", p.Disk, p.Zone),
		Labels:      map[string]string{snapshotLabel: diskID},
	}
}

// snapshotName is a valid, unique snapshot name: `<disk, shortened>-predelete-<unix time>`.
func (p Params) snapshotName(now int64) string {
	base := []rune(p.Disk)
	if len(base) > snapshotNameDiskLen {
		base = base[:snapshotNameDiskLen]
	}
	return fmt.Sprintf("%s-predelete-%d", strings.TrimRight(string(base), "-"), now)
}

// Output is the result reported to the caller.
type Output struct {
	Volume   *volume.Volume         `json:"volume,omitempty"`
	Snapshot *volume.SnapshotStatus `json:"snapshot,omitempty"`
	Deleted  bool                   `json:"deleted"`
	// Operation is the Compute Engine operation of the snapshot or deletion that was started.
	Operation string `json:"operation,omitempty"`
}
