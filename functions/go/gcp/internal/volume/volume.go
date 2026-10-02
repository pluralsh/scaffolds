package volume

import (
	"fmt"
	"strings"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// Volume is the volume to delete, as reported by the cloud.
type Volume struct {
	ID string `json:"id"`
	// State is the cloud-specific state, e.g. `available`, `READY` or `Unattached`.
	State string `json:"state"`
	// Deletable is whether the state allows deleting the volume.
	Deletable bool `json:"deletable"`
	// AttachedTo is whatever the volume is attached to or used by, e.g. instance IDs.
	AttachedTo []string `json:"attachedTo,omitempty"`
	SizeGiB    *int64   `json:"sizeGib,omitempty"`
	// Kubernetes is the PersistentVolumeClaim the volume was created for, if Kubernetes
	// created it.
	Kubernetes *KubernetesClaim `json:"kubernetes,omitempty"`
	// DetachedAt is when the volume was last detached (Unix seconds), if the cloud reports it.
	DetachedAt *int64 `json:"detachedAt,omitempty"`
}

// guards checks the volume itself, which should belong to the PersistentVolume pv.
func (v *Volume) guards(pv PVName) core.Guards {
	return core.Guards{
		core.Pass(guardExists, "found "+v.ID),
		v.unattachedGuard(),
		core.Check(guardState, v.Deletable, "state is "+v.State),
		v.kubernetesGuard(pv),
	}
}

func (v *Volume) unattachedGuard() core.Guard {
	if len(v.AttachedTo) == 0 {
		return core.Pass(guardUnattached, "not attached")
	}
	return core.Fail(guardUnattached, "attached to "+strings.Join(v.AttachedTo, ", "))
}

func (v *Volume) kubernetesGuard(pv PVName) core.Guard {
	claim := v.Kubernetes
	switch {
	case claim == nil:
		return core.Fail(guardKubernetes, "not created by Kubernetes for a PersistentVolumeClaim; only such volumes can be deleted")
	case claim.PV != string(pv):
		created := claim.PV
		if created == "" {
			created = "<unknown>"
		}
		return core.Fail(guardKubernetes, fmt.Sprintf("created for PersistentVolume %s, not %s", created, pv))
	}
	namespace := ""
	if claim.Namespace != "" {
		namespace = claim.Namespace + "/"
	}
	return core.Pass(guardKubernetes, fmt.Sprintf("created for PersistentVolume %s of PVC %s%s", pv, namespace, claim.PVC))
}

// KubernetesClaim identifies the PersistentVolumeClaim a volume was created for.
type KubernetesClaim struct {
	PVC       string `json:"pvc"`
	Namespace string `json:"namespace,omitempty"`
	PV        string `json:"pv,omitempty"`
}

// ClaimFromMetadata builds the claim from the standard `kubernetes.io/created-for/*`
// metadata that CSI drivers put on volumes. Empty values mean absent. It returns nil if there
// is no PVC name.
func ClaimFromMetadata(pvc, namespace, pv string) *KubernetesClaim {
	pvc = strings.TrimSpace(pvc)
	if pvc == "" {
		return nil
	}
	return &KubernetesClaim{
		PVC:       pvc,
		Namespace: strings.TrimSpace(namespace),
		PV:        strings.TrimSpace(pv),
	}
}

// PVName is the name of the PersistentVolume the caller expects the volume to belong to.
type PVName string

// Validate checks that the name is a Kubernetes object name (DNS-1123 subdomain).
func (n PVName) Validate() error {
	alnum := func(b byte) bool { return b >= 'a' && b <= 'z' || b >= '0' && b <= '9' }
	valid := len(n) >= 1 && len(n) <= 253 && alnum(n[0]) && alnum(n[len(n)-1])
	for i := 0; valid && i < len(n); i++ {
		valid = alnum(n[i]) || n[i] == '-' || n[i] == '.'
	}
	if !valid {
		return core.InvalidRequestf("pvName %q is not a PersistentVolume name", string(n))
	}
	return nil
}
