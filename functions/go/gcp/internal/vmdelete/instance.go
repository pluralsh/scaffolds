package vmdelete

import (
	"fmt"
	"slices"
	"strings"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// The names of the guards, in the order they are evaluated.
const (
	guardExists             = "exists"
	guardStandalone         = "standalone"
	guardNotGKE             = "not-gke"
	guardDeletionProtection = "deletion-protection"
	guardBootDisk           = "boot-disk"
	guardIdle               = "idle"
	guardAutoDelete         = "auto-delete"
)

const (
	// createdByKey is the metadata key Compute Engine sets on the instances a managed instance
	// group creates, holding the group manager's URL.
	createdByKey = "created-by"
	// groupManagersPath is the collection of instance group managers in a resource URL.
	groupManagersPath = "/instanceGroupManagers/"

	// gkeNodeLabel and gkeClusterLabel are the labels GKE sets on its nodes.
	gkeNodeLabel    = "goog-gke-node"
	gkeClusterLabel = "goog-k8s-cluster-name"

	// diskTypeScratch is the type of a local SSD, which is always deleted with the instance.
	diskTypeScratch = "SCRATCH"
)

// The instance statuses in which it isn't changing and can be deleted.
var idleStatuses = []string{"RUNNING", "TERMINATED", "SUSPENDED"}

// instance is a Compute Engine instance seen as the VM to delete.
type instance struct {
	*compute.Instance
}

// guards are the checks on the instance before it can be deleted.
func (i instance) guards() core.Guards {
	return core.Guards{
		core.Pass(guardExists, "Instance "+i.GetName()+" exists."),
		i.standaloneGuard(),
		i.gkeGuard(),
		i.deletionProtectionGuard(),
		i.bootDiskGuard(),
		core.Check(guardIdle, i.idle(), "The instance's status is "+i.GetStatus()+"."),
	}
}

func (i instance) standaloneGuard() core.Guard {
	if group := i.groupManager(); group != "" {
		return core.Fail(guardStandalone, fmt.Sprintf("Managed instance group %s created the instance. Resize the group instead, so it doesn't recreate the instance.", group))
	}
	return core.Pass(guardStandalone, "No managed instance group created the instance.")
}

func (i instance) gkeGuard() core.Guard {
	labels := i.GetLabels()
	if cluster, ok := labels[gkeClusterLabel]; ok {
		return core.Fail(guardNotGKE, fmt.Sprintf("The instance is a node of GKE cluster %s. Use node-pool-resize instead.", cluster))
	}
	if _, ok := labels[gkeNodeLabel]; ok {
		return core.Fail(guardNotGKE, fmt.Sprintf("GKE manages the instance (label %s). Use node-pool-resize instead.", gkeNodeLabel))
	}
	return core.Pass(guardNotGKE, "GKE doesn't manage the instance.")
}

func (i instance) bootDiskGuard() core.Guard {
	if i.bootDisk() == nil {
		return core.Fail(guardBootDisk, "The instance has no persistent boot disk, which is what is deleted with it.")
	}
	return core.Pass(guardBootDisk, "The boot disk is a persistent disk, deleted with the instance.")
}

func (i instance) deletionProtectionGuard() core.Guard {
	if i.GetDeletionProtection() {
		return core.Fail(guardDeletionProtection, "Deletion protection is enabled. Disable it first.")
	}
	return core.Pass(guardDeletionProtection, "Deletion protection is disabled.")
}

// groupManager is the name of the managed instance group that created the instance, or empty.
func (i instance) groupManager() string {
	for _, item := range i.GetMetadata().GetItems() {
		if item.GetKey() == createdByKey && strings.Contains(item.GetValue(), groupManagersPath) {
			return compute.NameFromURL(item.GetValue())
		}
	}
	return ""
}

func (i instance) idle() bool {
	return slices.Contains(idleStatuses, i.GetStatus())
}

// bootDisk is the persistent boot disk, or nil if there is none.
func (i instance) bootDisk() *compute.AttachedDisk {
	for _, disk := range i.GetDisks() {
		if disk.GetBoot() && disk.GetType() != diskTypeScratch {
			return disk
		}
	}
	return nil
}

// persistentDisks are the attached persistent disks, whose auto-delete flag can be changed.
func (i instance) persistentDisks() []*compute.AttachedDisk {
	var disks []*compute.AttachedDisk
	for _, disk := range i.GetDisks() {
		if disk.GetType() != diskTypeScratch {
			disks = append(disks, disk)
		}
	}
	return disks
}

// autoDeleteSet reports whether deleting the instance deletes its boot disk and keeps its
// data disks.
func (i instance) autoDeleteSet() bool {
	if i.bootDisk() == nil {
		return false
	}
	for _, disk := range i.persistentDisks() {
		if disk.GetAutoDelete() != disk.GetBoot() {
			return false
		}
	}
	return true
}

func (i instance) summary() *Summary {
	s := &Summary{
		Name:                 i.GetName(),
		State:                i.GetStatus(),
		InstanceGroupManager: i.groupManager(),
		DataDisks:            []string{},
	}
	for _, disk := range i.GetDisks() {
		switch {
		case disk.GetType() == diskTypeScratch:
			s.LocalSSDs = append(s.LocalSSDs, disk.GetDeviceName())
		case disk.GetBoot():
			s.BootDisk = compute.NameFromURL(disk.GetSource())
		default:
			s.DataDisks = append(s.DataDisks, compute.NameFromURL(disk.GetSource()))
		}
	}
	return s
}
