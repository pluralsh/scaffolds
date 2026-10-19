package lbfrontenddelete

import (
	"regexp"
	"strings"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

var (
	// lbName is the name the GKE cloud provider gives a Service's load balancer: `a` and the
	// first 31 hex digits of the Service UID without dashes.
	lbName = regexp.MustCompile(`^a[0-9a-f]{31}$`)
	// label is a Kubernetes namespace or Service name (an RFC 1123 label).
	label = regexp.MustCompile(`^[a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?$`)
)

// Params are the operation-specific fields of the tool input.
type Params struct {
	Region string `json:"region" required:"true"`
	// ForwardingRule is the name of the Service's load balancer, which all its resources share.
	ForwardingRule string `json:"forwardingRule" required:"true"`
	// ServiceName is the deleted Service as namespace/name.
	ServiceName string `json:"serviceName" required:"true"`
}

// Validate checks the parameters before anything is asked of Compute Engine.
func (p Params) Validate() error {
	if !compute.Zone(p.Region + "-a").Valid() {
		return core.InvalidRequestf("region %q is not a Compute Engine region (e.g. us-central1)", p.Region)
	}
	if !lbName.MatchString(p.ForwardingRule) {
		return core.InvalidRequestf("forwardingRule %q is not the name of a Service load balancer: a and the first 31 hex digits of the Service UID", p.ForwardingRule)
	}
	namespace, name, ok := strings.Cut(p.ServiceName, "/")
	if !ok || !label.MatchString(namespace) || !label.MatchString(name) {
		return core.InvalidRequestf("serviceName %q is not namespace/name", p.ServiceName)
	}
	return nil
}

// Output is the result reported to the caller.
type Output struct {
	// Resources are what is left of the load balancer, with what happens to each.
	Resources []Resource `json:"resources"`
	// Remaining is whether a later execute has more to delete.
	Remaining bool `json:"remaining"`
}

// Step is what happens to a resource.
type Step string

const (
	// StepDelete means execute deletes the resource (plan only).
	StepDelete Step = "delete"
	// StepDeleting means the deletion was started.
	StepDeleting Step = "deleting"
	// StepWaiting means Compute Engine refused to delete it yet, e.g. while a resource using it
	// is being deleted; execute again.
	StepWaiting Step = "waiting"
	// StepLater means a later execute deletes it, once nothing uses it.
	StepLater Step = "later"
	// StepKeep means the resource is kept: it wasn't created for the Service, or something
	// else uses it.
	StepKeep Step = "keep"
)

// Resource is a resource of the load balancer.
type Resource struct {
	Kind   compute.LBKind `json:"kind"`
	Name   string         `json:"name"`
	Step   Step           `json:"step"`
	Detail string         `json:"detail,omitempty"`
	// Operation is the Compute Engine operation of the deletion, once started.
	Operation string `json:"operation,omitempty"`
}
