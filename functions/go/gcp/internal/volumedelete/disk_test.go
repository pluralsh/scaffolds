package volumedelete

import (
	"testing"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volume"
)

func TestMapsCSIDiskMetadata(t *testing.T) {
	vol := disk{testDisk(csiDescription)}.volume()

	if !vol.Deletable || len(vol.AttachedTo) != 0 {
		t.Errorf("volume = %+v", vol)
	}
	if vol.SizeGiB == nil || *vol.SizeGiB != 10 {
		t.Errorf("size = %v", vol.SizeGiB)
	}
	if want := volume.ClaimFromMetadata("data", "apps", "pvc-123"); *vol.Kubernetes != *want {
		t.Errorf("kubernetes = %+v, want %+v", vol.Kubernetes, want)
	}
}

func TestMapsAttachedDiskWithoutKubernetesMetadata(t *testing.T) {
	for _, description := range []string{"hand-made disk", "", `{"a":1}`} {
		vol := disk{testDisk(description, instanceURL)}.volume()

		if len(vol.AttachedTo) != 1 || vol.AttachedTo[0] != "node-1" {
			t.Errorf("attachedTo = %v", vol.AttachedTo)
		}
		if vol.Kubernetes != nil {
			t.Errorf("%q: kubernetes = %+v", description, vol.Kubernetes)
		}
	}
}

func TestMapsDiskState(t *testing.T) {
	resource := testDisk("")
	resource.Status = new("CREATING")

	vol := disk{resource}.volume()

	if vol.State != "CREATING" || vol.Deletable {
		t.Errorf("volume = %+v", vol)
	}
}

func TestReadsLastDetachTime(t *testing.T) {
	resource := testDisk("")
	resource.LastDetachTimestamp = new("2026-09-30T03:00:00.000-07:00")

	vol := disk{resource}.volume()

	want := mustTimestamp(t, "2026-09-30T10:00:00Z")
	if vol.DetachedAt == nil || *vol.DetachedAt != want {
		t.Errorf("detachedAt = %v, want %d", vol.DetachedAt, want)
	}
	if got := (disk{testDisk("")}).volume().DetachedAt; got != nil {
		t.Errorf("detachedAt = %v, want none", *got)
	}
}
