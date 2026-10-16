package compute

import "errors"

// LBKind is a kind of load balancer resource.
type LBKind string

// The load balancer resources the functions read and delete.
const (
	// KindForwardingRule is a regional forwarding rule, the frontend of a load balancer.
	KindForwardingRule LBKind = "forwardingRule"
	// KindTargetPool is a regional target pool, the backend of an external passthrough load
	// balancer.
	KindTargetPool LBKind = "targetPool"
	// KindBackendService is a regional backend service, the backend of an internal passthrough
	// load balancer.
	KindBackendService LBKind = "backendService"
	// KindHTTPHealthCheck is a global legacy HTTP health check, used by target pools.
	KindHTTPHealthCheck LBKind = "httpHealthCheck"
	// KindHealthCheck is a global health check, used by backend services.
	KindHealthCheck LBKind = "healthCheck"
	// KindFirewall is a VPC firewall rule.
	KindFirewall LBKind = "firewall"
	// KindAddress is a regional static IP address.
	KindAddress LBKind = "address"
)

// ErrBusy is the cause of a deletion Compute Engine refused because the resource is still
// changing or in use, e.g. by a resource being deleted. A later attempt may succeed.
var ErrBusy = errors.New("resource busy")

// LBResource is a load balancer resource, with the fields of its kind that the functions use.
type LBResource struct {
	Kind        LBKind
	Name        string
	SelfLink    string
	Description string

	// Target is the URL of a forwarding rule's target pool or backend service.
	Target string
	// IPAddress is the address a forwarding rule serves.
	IPAddress string

	// Members are the instances of a target pool, or the instance groups of a backend
	// service's backends.
	Members []string
	// HealthChecks are the URLs of a target pool's or backend service's health checks.
	HealthChecks []string

	// Status and Users are an address's state (RESERVED or IN_USE) and what uses it.
	Status string
	Users  []string
}
