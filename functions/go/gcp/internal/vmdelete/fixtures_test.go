package vmdelete

import (
	"cloud.google.com/go/compute/apiv1/computepb"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
)

const diskURL = "https://www.googleapis.com/compute/v1/projects/p/zones/us-central1-a/disks/"

func testDisk(device string, boot, autoDelete bool) *compute.AttachedDisk {
	return &compute.AttachedDisk{
		DeviceName: new(device),
		Source:     new(diskURL + device),
		Boot:       new(boot),
		AutoDelete: new(autoDelete),
		Type:       new("PERSISTENT"),
	}
}

func testLocalSSD(device string) *compute.AttachedDisk {
	return &compute.AttachedDisk{DeviceName: new(device), AutoDelete: new(true), Type: new(diskTypeScratch)}
}

// testInstance is a standalone running instance whose boot disk is kept and data disk deleted,
// the opposite of what deleting it needs.
func testInstance() *compute.Instance {
	return &compute.Instance{
		Name:   new("vm-1"),
		Status: new("RUNNING"),
		Disks:  []*compute.AttachedDisk{testDisk("vm-1", true, false), testDisk("vm-1-data", false, true)},
	}
}

// withAutoDelete is the instance with the boot disk set to be deleted and the data disks kept.
func withAutoDelete(instance *compute.Instance) *compute.Instance {
	for _, disk := range instance.GetDisks() {
		if disk.GetType() != diskTypeScratch {
			disk.AutoDelete = new(disk.GetBoot())
		}
	}
	return instance
}

func withStatus(instance *compute.Instance, status string) *compute.Instance {
	instance.Status = &status
	return instance
}

func withMetadata(instance *compute.Instance, key, value string) *compute.Instance {
	instance.Metadata = &computepb.Metadata{Items: []*computepb.Items{{Key: &key, Value: &value}}}
	return instance
}
