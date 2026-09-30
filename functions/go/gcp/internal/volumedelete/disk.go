package volumedelete

import (
	"encoding/json"
	"strconv"
	"strings"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volume"
)

// The keys of the JSON object the CSI driver writes to the description of the disks it
// creates.
const (
	pvcNameKey      = "kubernetes.io/created-for/pvc/name"
	pvcNamespaceKey = "kubernetes.io/created-for/pvc/namespace"
	pvNameKey       = "kubernetes.io/created-for/pv/name"
)

// diskStatusReady is the status of a disk that can be deleted.
const diskStatusReady = "READY"

// disk is a Compute Engine disk seen as the volume to delete.
type disk struct {
	*compute.Disk
}

// id is the numeric ID of the disk, which unlike its name is never reused.
func (d disk) id() (string, error) {
	if d.Id == nil {
		return "", core.Providerf("compute API returned a disk without an ID")
	}
	return strconv.FormatUint(d.GetId(), 10), nil
}

// volume describes the disk as a volume.
func (d disk) volume() *volume.Volume {
	vol := &volume.Volume{
		ID:         d.GetName(),
		State:      d.GetStatus(),
		Deletable:  d.GetStatus() == diskStatusReady,
		AttachedTo: d.attachedTo(),
		SizeGiB:    d.SizeGb,
		Kubernetes: d.claim(),
	}
	if detached, ok := volume.ParseTimestamp(d.GetLastDetachTimestamp()); ok {
		vol.DetachedAt = &detached
	}
	return vol
}

// attachedTo lists the names of the instances using the disk.
func (d disk) attachedTo() []string {
	var instances []string
	for _, user := range d.GetUsers() {
		instances = append(instances, user[strings.LastIndexByte(user, '/')+1:])
	}
	return instances
}

// claim reads what Kubernetes recorded in the disk's description, if anything.
func (d disk) claim() *volume.KubernetesClaim {
	var metadata map[string]string
	if json.Unmarshal([]byte(d.GetDescription()), &metadata) != nil {
		return nil
	}
	return volume.ClaimFromMetadata(metadata[pvcNameKey], metadata[pvcNamespaceKey], metadata[pvNameKey])
}
