package lbfrontenddelete

import (
	"encoding/json"
	"fmt"
	"slices"
	"strings"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// The names of the guards, in the order they are evaluated.
const (
	guardExists    = "exists"
	guardService   = "service"
	guardTarget    = "service-load-balancer"
	guardUnhealthy = "no-healthy-backends"
)

// The description keys holding the Service a resource was created for.
var serviceNameKeys = []string{"kubernetes.io/service-name", "networking.gke.io/service-name"}

const (
	// addressInUse is the status of an address that something uses.
	addressInUse = "IN_USE"

	// The URL collections of the targets a Service's forwarding rule points at.
	targetPoolsPath     = "/targetPools/"
	backendServicesPath = "/backendServices/"
)

// leftovers are the resources of the load balancer that still exist.
type leftovers struct {
	service string

	rule         *compute.LBResource
	targets      []*compute.LBResource // the target pool and backend service named after the load balancer
	healthChecks []*compute.LBResource
	firewalls    []*compute.LBResource
	address      *compute.LBResource
	// ruleTarget is what the forwarding rule points at, which may be shared with other Services.
	ruleTarget *compute.LBResource
	// users are the other forwarding rules using a target, by target URL.
	users map[string]string
}

// empty reports whether nothing of the load balancer is left.
func (l *leftovers) empty() bool {
	return l.rule == nil && len(l.targets)+len(l.healthChecks)+len(l.firewalls) == 0 && l.address == nil
}

// owned reports whether the resource's description names the Service it was created for.
func (l *leftovers) owned(r *compute.LBResource) bool {
	return describedService(r.Description) == l.service
}

// describedService is the Service a resource's description names, or empty.
func describedService(description string) string {
	var fields map[string]any
	if json.Unmarshal([]byte(description), &fields) != nil {
		return ""
	}
	for _, key := range serviceNameKeys {
		if name, ok := fields[key].(string); ok {
			return name
		}
	}
	return ""
}

// guards are the checks before anything is deleted. healthy is a backend the target reports as
// healthy, or empty.
func (l *leftovers) guards(name, healthy string) core.Guards {
	if l.empty() {
		return core.Guards{core.Fail(guardExists, "Nothing of load balancer "+name+" is left.")}
	}
	guards := core.Guards{core.Pass(guardExists, "Load balancer "+name+" still has resources.")}
	if l.rule == nil {
		guards = append(guards, core.Pass(guardService, fmt.Sprintf("The forwarding rule is already deleted. Only resources created for Service %s are deleted.", l.service)))
	} else {
		guards = append(guards, l.serviceGuard(), l.targetGuard())
	}
	switch target := l.healthTarget(); {
	case target == nil:
		guards = append(guards, core.Pass(guardUnhealthy, "No target pool or backend service is left."))
	case healthy != "":
		guards = append(guards, core.Fail(guardUnhealthy, fmt.Sprintf(
			"%s %s reports %s as healthy. The Service may still exist, or the nodes still answer its health check.", target.Kind, target.Name, compute.NameFromURL(healthy))))
	default:
		guards = append(guards, core.Pass(guardUnhealthy, fmt.Sprintf("%s %s has no healthy backends.", target.Kind, target.Name)))
	}
	return guards
}

func (l *leftovers) serviceGuard() core.Guard {
	switch described := describedService(l.rule.Description); described {
	case l.service:
		return core.Pass(guardService, "The forwarding rule was created for Service "+l.service+".")
	case "":
		return core.Fail(guardService, "The forwarding rule's description names no Kubernetes Service.")
	default:
		return core.Fail(guardService, fmt.Sprintf("The forwarding rule was created for Service %s, not %s.", described, l.service))
	}
}

func (l *leftovers) targetGuard() core.Guard {
	target := l.rule.Target
	if strings.Contains(target, targetPoolsPath) || strings.Contains(target, backendServicesPath) {
		return core.Pass(guardTarget, "The forwarding rule forwards to "+compute.NameFromURL(target)+".")
	}
	return core.Fail(guardTarget, fmt.Sprintf("The forwarding rule forwards to %q, which isn't a target pool or backend service, so it isn't a Service load balancer.", target))
}

// healthTarget is the target whose backends must not be healthy: the forwarding rule's, or once
// it is gone, the one named after the load balancer.
func (l *leftovers) healthTarget() *compute.LBResource {
	if l.ruleTarget != nil {
		return l.ruleTarget
	}
	if len(l.targets) > 0 {
		return l.targets[0]
	}
	return nil
}

// steps decides what happens to each resource left.
func (l *leftovers) steps() []Resource {
	var steps []Resource
	add := func(r *compute.LBResource, step Step, detail string) {
		if !l.owned(r) {
			step, detail = StepKeep, "It wasn't created for Service "+l.service+"."
		}
		steps = append(steps, Resource{Kind: r.Kind, Name: r.Name, Step: step, Detail: detail})
	}
	afterRule := func() (Step, string) {
		if l.rule != nil {
			return StepLater, "A later execute deletes it, once the forwarding rule is gone."
		}
		return StepDelete, ""
	}

	if l.rule != nil {
		add(l.rule, StepDelete, "")
	}
	for _, target := range l.targets {
		step, detail := afterRule()
		if user, ok := l.users[target.SelfLink]; ok && step == StepDelete {
			step, detail = StepKeep, "Forwarding rule "+user+" uses it."
		}
		add(target, step, detail)
	}
	for _, firewall := range l.firewalls {
		step, detail := afterRule()
		add(firewall, step, detail)
	}
	if l.address != nil {
		step, detail := afterRule()
		if l.address.Status == addressInUse && step == StepDelete {
			step, detail = StepKeep, strings.Join(l.address.Users, ", ")+" uses it."
		}
		add(l.address, step, detail)
	}
	return append(steps, l.healthCheckSteps(steps)...)
}

// healthCheckSteps decides what happens to the health checks, which go once the targets using
// them are gone, given the steps of the other resources. A kept target keeps them.
func (l *leftovers) healthCheckSteps(steps []Resource) []Resource {
	targetsGoing := slices.ContainsFunc(steps, func(r Resource) bool {
		return (r.Kind == compute.KindTargetPool || r.Kind == compute.KindBackendService) && r.Step != StepKeep
	})
	var checks []Resource
	for _, check := range l.healthChecks {
		step, detail := StepDelete, ""
		switch {
		case !l.owned(check):
			step, detail = StepKeep, "It wasn't created for Service "+l.service+"."
		case l.rule != nil || targetsGoing:
			step, detail = StepLater, "A later execute deletes it, once the target pool or backend service is gone."
		case len(l.targets) > 0:
			step, detail = StepKeep, "A kept target pool or backend service may use it."
		}
		checks = append(checks, Resource{Kind: check.Kind, Name: check.Name, Step: step, Detail: detail})
	}
	return checks
}
