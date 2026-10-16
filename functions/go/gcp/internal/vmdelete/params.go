package vmdelete

import (
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// Params are the operation-specific fields of the tool input.
type Params struct {
	Zone     string `json:"zone" required:"true"`
	Instance string `json:"instance" required:"true"`
}

// Validate checks the parameters before anything is asked of Compute Engine.
func (p Params) Validate() error {
	if !compute.Zone(p.Zone).Valid() {
		return core.InvalidRequestf("zone %q is not a Compute Engine zone (e.g. us-central1-a)", p.Zone)
	}
	if !compute.ResourceName(p.Instance).Valid() {
		return core.InvalidRequestf("instance %q is not a Compute Engine instance name", p.Instance)
	}
	return nil
}

// Output is the result reported to the caller.
type Output struct {
	Instance *Summary `json:"instance,omitempty"`
	// AutoDeleteSet is whether deleting the instance now also deletes its boot disk and keeps
	// its data disks.
	AutoDeleteSet bool `json:"autoDeleteSet"`
	Deleted       bool `json:"deleted"`
	// Operation is the Compute Engine operation of the deletion, once started.
	Operation string `json:"operation,omitempty"`
}

// Summary describes the instance and what happens to its disks.
type Summary struct {
	Name  string `json:"name"`
	State string `json:"state"`
	// InstanceGroupManager is the managed instance group that created the instance, if any.
	InstanceGroupManager string `json:"instanceGroupManager,omitempty"`
	// BootDisk is deleted with the instance.
	BootDisk string `json:"bootDisk,omitempty"`
	// DataDisks are detached and kept.
	DataDisks []string `json:"dataDisks"`
	// LocalSSDs are deleted with the instance, with their data.
	LocalSSDs []string `json:"localSsds,omitempty"`
}
