package compute

import (
	"strings"
	"testing"
)

func TestValidatesZones(t *testing.T) {
	for _, zone := range []Zone{"us-central1-a", "europe-west4-b", "northamerica-northeast1-c", "me-central2-a"} {
		if !zone.Valid() {
			t.Errorf("Zone(%q).Valid() = false", zone)
		}
	}
	for _, zone := range []Zone{"", "us-central1", "us-central1-ab", "US-central1-a", "us-central-a", "us-1-a", "us-central1-a/../x"} {
		if zone.Valid() {
			t.Errorf("Zone(%q).Valid() = true", zone)
		}
	}
}

func TestValidatesResourceNames(t *testing.T) {
	if !ResourceName("pvc-0a1b2c3d").Valid() {
		t.Error("ResourceName(pvc-0a1b2c3d).Valid() = false")
	}
	for _, name := range []ResourceName{"", "Disk", "1disk", "disk-", "disk_1", "a/b", ResourceName(strings.Repeat("a", 64))} {
		if name.Valid() {
			t.Errorf("ResourceName(%q).Valid() = true", name)
		}
	}
}
