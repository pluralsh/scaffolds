package volumedelete

import (
	"testing"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volume"
)

const csiDescription = `{"kubernetes.io/created-for/pv/name":"pvc-123","kubernetes.io/created-for/pvc/name":"data","kubernetes.io/created-for/pvc/namespace":"apps","storage.gke.io/created-by":"pd.csi.storage.gke.io"}`

const instanceURL = "https://www.googleapis.com/compute/v1/projects/p/zones/z/instances/node-1"

func testDisk(description string, users ...string) *compute.Disk {
	return &compute.Disk{
		Id:          new(uint64(4242)),
		Name:        new("pvc-1"),
		Status:      new("READY"),
		SizeGb:      new(int64(10)),
		Users:       users,
		Description: &description,
	}
}

func testSnapshot(name, created, status string) *compute.Snapshot {
	return &compute.Snapshot{
		Name:              &name,
		Status:            &status,
		CreationTimestamp: &created,
		SourceDiskId:      new("4242"),
	}
}

func mustTimestamp(t *testing.T, value string) int64 {
	t.Helper()
	ts, ok := volume.ParseTimestamp(value)
	if !ok {
		t.Fatalf("invalid timestamp %q", value)
	}
	return ts
}
