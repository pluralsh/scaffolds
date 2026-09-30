package volumedelete

import (
	"strings"
	"testing"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
)

func TestValidatesParams(t *testing.T) {
	if err := (Params{Zone: "us-central1-a", Disk: "pvc-1", PVName: "pvc-1"}).Validate(); err != nil {
		t.Errorf("valid params rejected: %v", err)
	}
	for name, params := range map[string]Params{
		"zone":   {Zone: "us-central1", Disk: "pvc-1", PVName: "pvc-1"},
		"disk":   {Zone: "us-central1-a", Disk: "Disk_1", PVName: "pvc-1"},
		"pvName": {Zone: "us-central1-a", Disk: "pvc-1"},
	} {
		if err := params.Validate(); err == nil || !strings.Contains(err.Error(), name) {
			t.Errorf("invalid %s: err = %v", name, err)
		}
	}
}

func TestSnapshotNamesAreValid(t *testing.T) {
	snapshotName := func(disk string, now int64) string {
		return Params{Disk: disk}.snapshotName(now)
	}
	name := snapshotName(strings.Repeat("a", 63), 1_790_787_600)

	if !compute.ResourceName(name).Valid() {
		t.Errorf("%q is not a valid name", name)
	}
	if want := strings.Repeat("a", 40) + "-predelete-1790787600"; name != want {
		t.Errorf("name = %q, want %q", name, want)
	}
	if got := snapshotName("pvc-1", 1_790_787_600); got != "pvc-1-predelete-1790787600" {
		t.Errorf("name = %q", got)
	}
	// Trailing dashes are trimmed where the name is cut, so the name has no double dash.
	if got := snapshotName(strings.Repeat("a", 39)+"-b", 1); got != strings.Repeat("a", 39)+"-predelete-1" {
		t.Errorf("name = %q", got)
	}
}
