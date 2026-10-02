package lbfrontenddelete

import (
	"context"
	"errors"
	"log/slog"
	"slices"
	"strings"
	"sync"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// operation is a single invocation of the function, after its input was validated.
type operation struct {
	client compute.LoadBalancers
	action core.Action
	params Params
}

// run finds what is left of the load balancer, evaluates the guards and deletes what they allow.
func (o *operation) run(ctx context.Context) (core.Response[Output], error) {
	left, err := o.leftovers(ctx)
	if err != nil {
		return core.Response[Output]{}, err
	}
	healthy, err := o.healthyBackend(ctx, left.healthTarget())
	if err != nil {
		return core.Response[Output]{}, err
	}

	guards := left.guards(o.params.ForwardingRule, healthy)
	steps := left.steps()
	if o.action == core.ActionPlan || !guards.Passed() {
		output := Output{Resources: steps, Remaining: remaining(steps, StepDelete, StepLater)}
		if o.action == core.ActionPlan {
			return core.Planned(guards, output), nil
		}
		return core.Refused[Output](guards).WithResult(output), nil
	}

	for i := range steps {
		if steps[i].Step != StepDelete {
			continue
		}
		if err := o.delete(ctx, &steps[i]); err != nil {
			return core.Response[Output]{}, err
		}
	}
	return core.Done(guards, Output{Resources: steps, Remaining: remaining(steps, StepWaiting, StepLater)}), nil
}

// leftovers reads every resource the GKE cloud provider creates for the load balancer.
func (o *operation) leftovers(ctx context.Context) (*leftovers, error) {
	name, region := o.params.ForwardingRule, o.params.Region
	left := &leftovers{service: o.params.ServiceName}
	get := func(kind compute.LBKind, name string) (*compute.LBResource, error) {
		return o.client.LBResource(ctx, kind, region, name)
	}
	var err error
	if left.rule, err = get(compute.KindForwardingRule, name); err != nil {
		return nil, err
	}
	for _, kind := range []compute.LBKind{compute.KindTargetPool, compute.KindBackendService} {
		target, err := get(kind, name)
		if err != nil {
			return nil, err
		}
		if target != nil {
			left.targets = append(left.targets, target)
		}
	}
	for _, kind := range []compute.LBKind{compute.KindHTTPHealthCheck, compute.KindHealthCheck} {
		check, err := get(kind, name)
		if err != nil {
			return nil, err
		}
		if check != nil {
			left.healthChecks = append(left.healthChecks, check)
		}
	}
	for _, firewall := range []string{"k8s-fw-" + name, "k8s-" + name + "-http-hc"} {
		rule, err := get(compute.KindFirewall, firewall)
		if err != nil {
			return nil, err
		}
		if rule != nil {
			left.firewalls = append(left.firewalls, rule)
		}
	}
	if left.address, err = get(compute.KindAddress, name); err != nil {
		return nil, err
	}

	if left.rule != nil {
		left.ruleTarget, err = o.ruleTarget(ctx, left)
	} else if len(left.targets) > 0 {
		left.users, err = o.targetUsers(ctx)
	}
	if err != nil {
		return nil, err
	}
	return left, nil
}

// ruleTarget is the target pool or backend service the forwarding rule points at, or nil.
func (o *operation) ruleTarget(ctx context.Context, left *leftovers) (*compute.LBResource, error) {
	url := left.rule.Target
	if i := slices.IndexFunc(left.targets, func(t *compute.LBResource) bool { return t.SelfLink == url }); i >= 0 {
		return left.targets[i], nil
	}
	switch {
	case strings.Contains(url, targetPoolsPath):
		return o.client.LBResource(ctx, compute.KindTargetPool, compute.RegionFromURL(url), compute.NameFromURL(url))
	case strings.Contains(url, backendServicesPath):
		return o.client.LBResource(ctx, compute.KindBackendService, compute.RegionFromURL(url), compute.NameFromURL(url))
	}
	return nil, nil
}

// targetUsers are the forwarding rules of the region by the URL of their target.
func (o *operation) targetUsers(ctx context.Context) (map[string]string, error) {
	rules, err := o.client.ForwardingRules(ctx, o.params.Region)
	if err != nil {
		return nil, err
	}
	users := make(map[string]string, len(rules))
	for _, rule := range rules {
		users[rule.Target] = rule.Name
	}
	return users, nil
}

// healthChecks is how many backends are asked about their health at once.
const healthChecks = 10

// healthyBackend is a backend the target reports as healthy, or empty if none is. It asks
// about several backends at once and stops at the first healthy one.
func (o *operation) healthyBackend(ctx context.Context, target *compute.LBResource) (string, error) {
	if target == nil || len(target.Members) == 0 {
		return "", nil
	}
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	type answer struct {
		member  string
		healthy bool
		err     error
	}
	members := make(chan string)
	answers := make(chan answer)
	var workers sync.WaitGroup
	for range min(healthChecks, len(target.Members)) {
		workers.Go(func() {
			for member := range members {
				healthy, err := o.client.Healthy(ctx, target, member)
				answers <- answer{member, healthy, err}
			}
		})
	}
	go func() {
		defer close(members)
		for _, member := range target.Members {
			select {
			case members <- member:
			case <-ctx.Done():
				return
			}
		}
	}()
	go func() {
		workers.Wait()
		close(answers)
	}()

	var healthy string
	var firstErr error
	for a := range answers {
		switch {
		case a.err != nil && firstErr == nil && healthy == "":
			firstErr = a.err
			cancel()
		case a.healthy && healthy == "" && firstErr == nil:
			healthy = a.member
			cancel()
		}
	}
	return healthy, firstErr
}

// delete starts deleting the resource of the step and records the outcome in it.
func (o *operation) delete(ctx context.Context, step *Resource) error {
	op, err := o.client.DeleteLBResource(ctx, step.Kind, o.params.Region, step.Name)
	switch {
	case errors.Is(err, compute.ErrBusy):
		step.Step, step.Detail = StepWaiting, err.Error()
		return nil
	case err != nil:
		return err
	}
	slog.InfoContext(ctx, "deleting load balancer resource", "kind", string(step.Kind), "name", step.Name,
		"service", o.params.ServiceName, "operation", op)
	step.Step, step.Operation = StepDeleting, op
	return nil
}

// remaining reports whether any resource is at one of the steps.
func remaining(steps []Resource, pending ...Step) bool {
	return slices.ContainsFunc(steps, func(r Resource) bool { return slices.Contains(pending, r.Step) })
}
