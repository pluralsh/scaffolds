package nodepoolresize

import (
	"context"
	"log/slog"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gke"
)

// operation is a single invocation of the function, after its input was validated.
type operation struct {
	clusters gke.Client
	groups   compute.Groups
	action   core.Action
	params   Params
	maxCount int64
}

// run inspects the pool, evaluates the guards and resizes it if they allow.
func (o *operation) run(ctx context.Context) (core.Response[Output], error) {
	target, err := o.pool(ctx)
	if err != nil {
		return core.Response[Output]{}, err
	}
	if target == nil {
		return o.refuse(core.Guards{core.Fail(guardExists, "cluster or node pool not found")}, Output{To: o.params.Count}), nil
	}

	guards := target.guards(o.params.Count, o.maxCount)
	output := Output{
		NodePool: target.summary(),
		From:     target.total(),
		To:       o.params.Count * int64(len(target.zones())),
	}
	if o.action == core.ActionPlan || !guards.Passed() {
		return o.refuse(guards, output), nil
	}
	if target.sized(o.params.Count) {
		return core.Done(guards, output), nil
	}

	op, err := o.clusters.SetNodePoolSize(ctx, o.params.Location, o.params.Cluster, o.params.NodePool, o.params.Count)
	if err != nil {
		return core.Response[Output]{}, err
	}
	slog.InfoContext(ctx, "resizing node pool", "location", o.params.Location, "cluster", o.params.Cluster,
		"node_pool", o.params.NodePool, "count", o.params.Count, "operation", op)
	output.Submitted = true
	output.Operation = op
	return core.Done(guards, output), nil
}

// refuse answers a plan, or an execute that changes nothing.
func (o *operation) refuse(guards core.Guards, output Output) core.Response[Output] {
	if o.action == core.ActionPlan {
		return core.Planned(guards, output)
	}
	return core.Refused[Output](guards).WithResult(output)
}

// pool reads the cluster, the node pool and the current size of each of its zones, or returns
// nil if the cluster or pool doesn't exist.
func (o *operation) pool(ctx context.Context) (*pool, error) {
	cluster, err := o.clusters.Cluster(ctx, o.params.Location, o.params.Cluster)
	if err != nil || cluster == nil {
		return nil, err
	}
	for _, nodePool := range cluster.NodePools {
		if nodePool.Name != o.params.NodePool {
			continue
		}
		sizes, err := o.sizes(ctx, nodePool.InstanceGroupUrls)
		if err != nil {
			return nil, err
		}
		return &pool{NodePool: nodePool, cluster: cluster, sizes: sizes}, nil
	}
	return nil, nil
}

// sizes are the target sizes of the pool's managed instance groups, by zone.
func (o *operation) sizes(ctx context.Context, groupURLs []string) (map[string]int64, error) {
	sizes := make(map[string]int64, len(groupURLs))
	for _, url := range groupURLs {
		zone := compute.ZoneFromURL(url)
		group, err := o.groups.InstanceGroupManager(ctx, zone, compute.NameFromURL(url))
		if err != nil {
			return nil, err
		}
		if group != nil {
			sizes[zone] += int64(group.GetTargetSize())
		}
	}
	return sizes, nil
}
