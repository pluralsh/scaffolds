package nodepoolresize

import (
	"fmt"
	"maps"
	"slices"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gke"
)

// The names of the guards, in the order they are evaluated.
const (
	guardExists         = "exists"
	guardStandard       = "standard-cluster"
	guardIdle           = "idle"
	guardManuallyScaled = "manually-scaled"
	guardCountAllowed   = "count-allowed"
)

// statusRunning is the status of a cluster or node pool that no operation is changing.
const statusRunning = "RUNNING"

// pool is a GKE node pool seen as the pool to resize.
type pool struct {
	*gke.NodePool
	cluster *gke.Cluster
	// sizes are the current node counts by zone, read from the pool's instance groups. A zone
	// whose group couldn't be found is missing.
	sizes map[string]int64
}

// guards are the checks on the pool before it is resized to count nodes per zone, with at
// most maxCount nodes in total.
func (p pool) guards(count, maxCount int64) core.Guards {
	return core.Guards{
		core.Pass(guardExists, fmt.Sprintf("node pool %s of cluster %s", p.Name, p.cluster.Name)),
		core.Check(guardStandard, !p.autopilot(), "Standard cluster; Autopilot clusters manage their nodes themselves"),
		core.Check(guardIdle, p.cluster.Status == statusRunning && p.Status == statusRunning,
			fmt.Sprintf("cluster %s, node pool %s; another operation must finish first unless both are %s", p.cluster.Status, p.Status, statusRunning)),
		p.manualGuard(),
		p.countGuard(count, maxCount),
	}
}

func (p pool) autopilot() bool {
	return p.cluster.Autopilot != nil && p.cluster.Autopilot.Enabled
}

func (p pool) autoscaling() bool {
	return p.Autoscaling != nil && p.Autoscaling.Enabled
}

func (p pool) manualGuard() core.Guard {
	if !p.autoscaling() {
		return core.Pass(guardManuallyScaled, "cluster autoscaler disabled")
	}
	a := p.Autoscaling
	if a.TotalMaxNodeCount > 0 {
		return core.Fail(guardManuallyScaled, fmt.Sprintf(
			"the cluster autoscaler scales this pool between %d and %d nodes in total; change those instead", a.TotalMinNodeCount, a.TotalMaxNodeCount))
	}
	return core.Fail(guardManuallyScaled, fmt.Sprintf(
		"the cluster autoscaler scales this pool between %d and %d nodes per zone; change those instead", a.MinNodeCount, a.MaxNodeCount))
}

func (p pool) countGuard(count, maxCount int64) core.Guard {
	zones := int64(len(p.zones()))
	var minCount int64
	reason := ""
	if p.only() {
		minCount = 1
		reason = "; the cluster's only node pool keeps a node per zone"
	}
	total := count * zones
	return core.Check(guardCountAllowed, count >= minCount && total <= maxCount, fmt.Sprintf(
		"%d nodes per zone in %d zones, %d in total; the installation allows at most %d in total%s", count, zones, total, maxCount, reason))
}

// only reports whether the pool is the cluster's only node pool.
func (p pool) only() bool {
	return len(p.cluster.NodePools) == 1
}

// zones are the zones the pool has nodes in: its own, or the cluster's by default.
func (p pool) zones() []string {
	if len(p.Locations) > 0 {
		return p.Locations
	}
	return p.cluster.Locations
}

// total is the current node count of the pool, if every zone's count is known.
func (p pool) total() *int64 {
	var total int64
	for _, zone := range p.zones() {
		size, ok := p.sizes[zone]
		if !ok {
			return nil
		}
		total += size
	}
	return &total
}

// sized reports whether every zone already has count nodes.
func (p pool) sized(count int64) bool {
	for _, zone := range p.zones() {
		if size, ok := p.sizes[zone]; !ok || size != count {
			return false
		}
	}
	return true
}

func (p pool) summary() *Summary {
	s := &Summary{
		Name:        p.Name,
		State:       p.Status,
		Autoscaling: p.autoscaling(),
		Zones:       slices.Clone(p.zones()),
	}
	if p.Config != nil {
		s.MachineType = p.Config.MachineType
	}
	if len(p.sizes) > 0 {
		s.NodesPerZone = maps.Clone(p.sizes)
	}
	return s
}
