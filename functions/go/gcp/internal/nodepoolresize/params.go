package nodepoolresize

import (
	"os"
	"strconv"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gke"
)

const (
	// MaxCountVar is the environment variable with the largest total node count callers may
	// set, at most [gkeMaxCount].
	MaxCountVar = "MAX_NODE_COUNT"

	// gkeMaxCount is the largest node count GKE allows in a node pool.
	gkeMaxCount = 1000
)

// MaxCountFromEnv is the cap set by [MaxCountVar], or [gkeMaxCount] if it is unset or invalid.
func MaxCountFromEnv() int64 {
	limit, err := strconv.ParseInt(os.Getenv(MaxCountVar), 10, 64)
	if err != nil || limit < 0 {
		return gkeMaxCount
	}
	return min(limit, gkeMaxCount)
}

// Params are the operation-specific fields of the tool input.
type Params struct {
	// Location is the region of a regional cluster or the zone of a zonal one.
	Location string `json:"location" required:"true"`
	Cluster  string `json:"cluster" required:"true"`
	NodePool string `json:"nodePool" required:"true"`
	// Count is the new number of nodes in each zone of the pool.
	Count int64 `json:"count" required:"true"`
}

// Validate checks the parameters before anything is asked of GKE.
func (p Params) Validate() error {
	if !gke.Location(p.Location).Valid() {
		return core.InvalidRequestf("location %q is not a region or zone (e.g. us-central1 or us-central1-a)", p.Location)
	}
	if !gke.Name(p.Cluster).Valid() {
		return core.InvalidRequestf("cluster %q is not a GKE cluster name", p.Cluster)
	}
	if !gke.Name(p.NodePool).Valid() {
		return core.InvalidRequestf("nodePool %q is not a GKE node pool name", p.NodePool)
	}
	if p.Count < 0 || p.Count > gkeMaxCount {
		return core.InvalidRequestf("count %d is not between 0 and %d", p.Count, gkeMaxCount)
	}
	return nil
}

// Output is the result reported to the caller.
type Output struct {
	NodePool *Summary `json:"nodePool,omitempty"`
	// From is the pool's current total node count, if every zone's count is known.
	From *int64 `json:"from,omitempty"`
	// To is the pool's total node count once resized: count in each zone.
	To int64 `json:"to"`
	// Submitted is whether the new count was submitted. GKE then adds or drains nodes in the
	// background.
	Submitted bool `json:"submitted"`
	// Operation is the GKE operation of the resize, once submitted.
	Operation string `json:"operation,omitempty"`
}

// Summary describes the node pool.
type Summary struct {
	Name        string `json:"name"`
	MachineType string `json:"machineType,omitempty"`
	State       string `json:"state"`
	Autoscaling bool   `json:"autoscaling"`
	// Zones are the zones the pool has nodes in.
	Zones []string `json:"zones"`
	// NodesPerZone is the current node count of each zone, as far as it is known.
	NodesPerZone map[string]int64 `json:"nodesPerZone,omitempty"`
}
