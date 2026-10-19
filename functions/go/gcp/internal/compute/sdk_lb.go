package compute

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"slices"

	computev1 "cloud.google.com/go/compute/apiv1"
	"cloud.google.com/go/compute/apiv1/computepb"
	computeapi "google.golang.org/api/compute/v1"
	"google.golang.org/api/googleapi"
	"google.golang.org/api/iterator"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// healthy is the health state of a healthy backend.
const healthy = "HEALTHY"

// The reasons Compute Engine gives for refusing to change a busy resource.
var busyReasons = []string{"resourceNotReady", "resourceInUseByAnotherResource"}

// lbClients are the SDK clients of the load balancer resources.
type lbClients struct {
	forwardingRules  *computev1.ForwardingRulesClient
	targetPools      *computev1.TargetPoolsClient
	backendServices  *computev1.RegionBackendServicesClient
	httpHealthChecks *computeapi.HttpHealthChecksService
	healthChecks     *computev1.HealthChecksClient
	firewalls        *computev1.FirewallsClient
	addresses        *computev1.AddressesClient
}

func (c *sdkClient) LBResource(ctx context.Context, kind LBKind, region, name string) (*LBResource, error) {
	var resource *LBResource
	var err error
	switch kind {
	case KindForwardingRule:
		var rule *computepb.ForwardingRule
		rule, err = c.lb.forwardingRules.Get(ctx, &computepb.GetForwardingRuleRequest{Project: c.project, Region: region, ForwardingRule: name})
		resource = fromForwardingRule(rule)
	case KindTargetPool:
		var pool *computepb.TargetPool
		pool, err = c.lb.targetPools.Get(ctx, &computepb.GetTargetPoolRequest{Project: c.project, Region: region, TargetPool: name})
		resource = &LBResource{Name: pool.GetName(), SelfLink: pool.GetSelfLink(), Description: pool.GetDescription(),
			Members: pool.GetInstances(), HealthChecks: pool.GetHealthChecks()}
	case KindBackendService:
		var service *computepb.BackendService
		service, err = c.lb.backendServices.Get(ctx, &computepb.GetRegionBackendServiceRequest{Project: c.project, Region: region, BackendService: name})
		resource = &LBResource{Name: service.GetName(), SelfLink: service.GetSelfLink(), Description: service.GetDescription(),
			HealthChecks: service.GetHealthChecks()}
		for _, backend := range service.GetBackends() {
			resource.Members = append(resource.Members, backend.GetGroup())
		}
	case KindHTTPHealthCheck:
		var check *computeapi.HttpHealthCheck
		check, err = c.lb.httpHealthChecks.Get(c.project, name).Context(ctx).Do()
		if check != nil {
			resource = &LBResource{Name: check.Name, SelfLink: check.SelfLink, Description: check.Description}
		}
	case KindHealthCheck:
		var check *computepb.HealthCheck
		check, err = c.lb.healthChecks.Get(ctx, &computepb.GetHealthCheckRequest{Project: c.project, HealthCheck: name})
		resource = &LBResource{Name: check.GetName(), SelfLink: check.GetSelfLink(), Description: check.GetDescription()}
	case KindFirewall:
		var rule *computepb.Firewall
		rule, err = c.lb.firewalls.Get(ctx, &computepb.GetFirewallRequest{Project: c.project, Firewall: name})
		resource = &LBResource{Name: rule.GetName(), SelfLink: rule.GetSelfLink(), Description: rule.GetDescription()}
	case KindAddress:
		var address *computepb.Address
		address, err = c.lb.addresses.Get(ctx, &computepb.GetAddressRequest{Project: c.project, Region: region, Address: name})
		resource = &LBResource{Name: address.GetName(), SelfLink: address.GetSelfLink(), Description: address.GetDescription(),
			IPAddress: address.GetAddress(), Status: address.GetStatus(), Users: address.GetUsers()}
	default:
		return nil, fmt.Errorf("unknown load balancer resource kind %q", kind)
	}
	if resource, err = found(resource, err); resource != nil {
		resource.Kind = kind
	}
	return resource, err
}

func (c *sdkClient) ForwardingRules(ctx context.Context, region string) ([]*LBResource, error) {
	it := c.lb.forwardingRules.List(ctx, &computepb.ListForwardingRulesRequest{Project: c.project, Region: region})
	var rules []*LBResource
	for {
		rule, err := it.Next()
		if errors.Is(err, iterator.Done) {
			return rules, nil
		}
		if err != nil {
			return nil, sdkError(err)
		}
		rules = append(rules, fromForwardingRule(rule))
	}
}

func (c *sdkClient) Healthy(ctx context.Context, target *LBResource, member string) (bool, error) {
	var statuses []*computepb.HealthStatus
	switch target.Kind {
	case KindTargetPool:
		health, err := c.lb.targetPools.GetHealth(ctx, &computepb.GetHealthTargetPoolRequest{
			Project:                   c.project,
			Region:                    RegionFromURL(target.SelfLink),
			TargetPool:                target.Name,
			InstanceReferenceResource: &computepb.InstanceReference{Instance: &member},
		})
		if gone(err) {
			return false, nil
		}
		if err != nil {
			return false, sdkError(err)
		}
		statuses = health.GetHealthStatus()
	case KindBackendService:
		health, err := c.lb.backendServices.GetHealth(ctx, &computepb.GetHealthRegionBackendServiceRequest{
			Project:                        c.project,
			Region:                         RegionFromURL(target.SelfLink),
			BackendService:                 target.Name,
			ResourceGroupReferenceResource: &computepb.ResourceGroupReference{Group: &member},
		})
		if gone(err) {
			return false, nil
		}
		if err != nil {
			return false, sdkError(err)
		}
		statuses = health.GetHealthStatus()
	default:
		return false, fmt.Errorf("%s %s has no backend health", target.Kind, target.Name)
	}
	return slices.ContainsFunc(statuses, func(s *computepb.HealthStatus) bool { return s.GetHealthState() == healthy }), nil
}

func (c *sdkClient) DeleteLBResource(ctx context.Context, kind LBKind, region, name string) (string, error) {
	var op *computev1.Operation
	var err error
	switch kind {
	case KindForwardingRule:
		op, err = c.lb.forwardingRules.Delete(ctx, &computepb.DeleteForwardingRuleRequest{Project: c.project, Region: region, ForwardingRule: name})
	case KindTargetPool:
		op, err = c.lb.targetPools.Delete(ctx, &computepb.DeleteTargetPoolRequest{Project: c.project, Region: region, TargetPool: name})
	case KindBackendService:
		op, err = c.lb.backendServices.Delete(ctx, &computepb.DeleteRegionBackendServiceRequest{Project: c.project, Region: region, BackendService: name})
	case KindHTTPHealthCheck:
		return c.deleteHTTPHealthCheck(ctx, name)
	case KindHealthCheck:
		op, err = c.lb.healthChecks.Delete(ctx, &computepb.DeleteHealthCheckRequest{Project: c.project, HealthCheck: name})
	case KindFirewall:
		op, err = c.lb.firewalls.Delete(ctx, &computepb.DeleteFirewallRequest{Project: c.project, Firewall: name})
	case KindAddress:
		op, err = c.lb.addresses.Delete(ctx, &computepb.DeleteAddressRequest{Project: c.project, Region: region, Address: name})
	default:
		return "", fmt.Errorf("unknown load balancer resource kind %q", kind)
	}
	if err != nil {
		if busy(err) {
			return "", fmt.Errorf("%w: %w", ErrBusy, err)
		}
		return "", sdkError(err)
	}
	return operationName(op)
}

// deleteHTTPHealthCheck starts deleting a legacy HTTP health check, which only the Google API
// client serves.
func (c *sdkClient) deleteHTTPHealthCheck(ctx context.Context, name string) (string, error) {
	op, err := c.lb.httpHealthChecks.Delete(c.project, name).Context(ctx).Do()
	switch {
	case err != nil && busy(err):
		return "", fmt.Errorf("%w: %w", ErrBusy, err)
	case err != nil:
		return "", sdkError(err)
	case op.Name == "":
		return "", core.Providerf("compute API returned an operation without a name")
	}
	return op.Name, nil
}

func fromForwardingRule(rule *computepb.ForwardingRule) *LBResource {
	target := rule.GetTarget()
	if target == "" {
		target = rule.GetBackendService()
	}
	return &LBResource{Kind: KindForwardingRule, Name: rule.GetName(), SelfLink: rule.GetSelfLink(),
		Description: rule.GetDescription(), Target: target, IPAddress: rule.GetIPAddress()}
}

// gone reports whether a health check failed because the member no longer exists, e.g. a node
// of a deleted cluster still listed in a target pool: Compute Engine answers 404, or 400 for an
// instance it can't resolve.
func gone(err error) bool {
	apiErr, ok := errors.AsType[*googleapi.Error](err)
	return ok && (apiErr.Code == http.StatusNotFound || apiErr.Code == http.StatusBadRequest)
}

// busy reports whether Compute Engine refused a change because the resource is busy or in use.
func busy(err error) bool {
	apiErr, ok := errors.AsType[*googleapi.Error](err)
	if !ok {
		return false
	}
	if apiErr.Code == http.StatusConflict {
		return true
	}
	return apiErr.Code == http.StatusBadRequest && slices.ContainsFunc(apiErr.Errors, func(item googleapi.ErrorItem) bool {
		return slices.Contains(busyReasons, item.Reason)
	})
}
