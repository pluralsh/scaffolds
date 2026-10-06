// Package iam is the IAM policy of a resource as the functions edit it, independent of the API
// that serves it.
package iam

// Version is the policy version read and written: the first one whose bindings can have
// conditions.
const Version = 3

// Policy is the IAM policy of a resource.
type Policy struct {
	// Etag is sent back when writing the policy, so a policy changed since it was read isn't
	// overwritten.
	Etag     string
	Bindings []*Binding
}

// Binding grants a role to members, if its condition holds.
type Binding struct {
	Role      string
	Members   []string
	Condition *Condition
}

// Condition is the CEL condition of a binding.
type Condition struct {
	Title       string
	Description string
	Expression  string
}
